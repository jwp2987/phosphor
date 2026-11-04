use std::time::Duration;

use super::{
    FailReason, MAX_RECOVERY_ATTEMPTS, PendingResume, RecoveryAction, RecoveryBudget,
    ResponseStream, ResponseStreamId, recovery_action, recovery_backoff_after_attempts,
    recovery_wait,
};
use super::{RetryWaitKind, clear_retry_wait, current_retry_wait, set_retry_wait};
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::api_error::AIApiError;
// `agent_sdk` (and so the driver's recovery deadline) is native-only here.
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

/// Fork-authored, added after a refutation pass found the coverage above insufficient.
///
/// `a_scheduled_resume_inherits_a_charged_budget` asserts arithmetic on `RecoveryBudget`
/// directly and never touches `PendingResume`, so gutting `PendingResume::recovery()` to
/// return `RecoveryBudget::fresh()` -- which would restore the exact per-path budget
/// REMOTE-2269 removed -- left the whole suite green. This pins the accessors against that.
///
/// What this does NOT cover, deliberately rather than by oversight: the threading from
/// `retry()`'s `pending_resume` assignment through `schedule_auto_resume_after_error` to
/// `ResponseStream::new`. That path needs a live stream and a `ModelContext`, so it wants
/// an integration seam this module does not have. Recorded in `TODO.md` rather than faked.
#[test]
fn pending_resume_carries_its_budget_and_backoff_verbatim() {
    // Built exactly as the `RecoveryAction::Resume` arm builds it: the stream's budget
    // charged one more attempt, and the backoff for that attempt number.
    let stream_budget = RecoveryBudget::fresh().next_attempt();
    let backoff = recovery_backoff_after_attempts(stream_budget.attempts_used() + 1);
    let pending = PendingResume {
        recovery: stream_budget.next_attempt(),
        backoff,
    };

    assert_eq!(
        pending.recovery().attempts_used(),
        2,
        "the resume must run with the stream's budget charged, not a fresh one"
    );
    assert_ne!(
        pending.recovery(),
        RecoveryBudget::fresh(),
        "a fresh budget here restores the per-path budget REMOTE-2269 removed"
    );
    assert_eq!(
        pending.backoff(),
        backoff,
        "the resume must wait the charged backoff"
    );

    // The immediate (BYOP bad-arguments) constructor is the one deliberate exception: a
    // fresh budget and no wait, bounded by its call site instead of by the budget.
    let immediate = PendingResume::immediate(RecoveryBudget::fresh());
    assert_eq!(immediate.backoff(), Duration::ZERO);
    assert_eq!(immediate.recovery().attempts_used(), 0);
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
#[test]
fn the_recovery_backoff_fits_inside_the_driver_run_recovery_window() {
    // The driver re-arms AUTO_RESUME_TIMEOUT per recovery attempt, so what has to fit in
    // that window is a single attempt's wait, not the whole chain. Assert both anyway: if
    // the budget or the backoff schedule ever grows enough to approach the deadline, a run
    // would start dying on the deadline instead of on the failure it was recovering from.
    let total: Duration = (1..=MAX_RECOVERY_ATTEMPTS)
        .map(recovery_backoff_after_attempts)
        .sum();
    assert!(
        total * 2 < AUTO_RESUME_TIMEOUT,
        "total recovery backoff {total:?} is too close to AUTO_RESUME_TIMEOUT {AUTO_RESUME_TIMEOUT:?}"
    );
    for attempt in 1..=MAX_RECOVERY_ATTEMPTS {
        assert!(recovery_backoff_after_attempts(attempt) * 4 < AUTO_RESUME_TIMEOUT);
    }
}

/// #854: the provider's `Retry-After` stretches the wait but never shortens it, and the
/// largest hint we accept still fits the driver's recovery window.
#[test]
fn retry_after_stretches_but_never_shortens_the_recovery_wait() {
    let backoff = Duration::from_secs(2);
    assert_eq!(recovery_wait(backoff, None), backoff);
    assert_eq!(
        recovery_wait(backoff, Some(Duration::from_secs(1))),
        backoff
    );
    assert_eq!(
        recovery_wait(backoff, Some(Duration::from_secs(30))),
        Duration::from_secs(30)
    );
    assert!(
        recovery_wait(backoff, Some(crate::ai::api_error::MAX_RETRY_AFTER)) * 2
            <= AUTO_RESUME_TIMEOUT
    );
}

/// #853/#854 follow-up: the countdown reads the recorded wait, rounds up to whole seconds, and
/// is gone once cleared or expired.
#[test]
fn retry_wait_registry_reports_then_clears() {
    let id = AIConversationId::new();
    assert_eq!(current_retry_wait(id), None);
    set_retry_wait(id, RetryWaitKind::RateLimited, Duration::from_secs(30));
    let (kind, secs) = current_retry_wait(id).expect("wait is recorded");
    assert_eq!(kind, RetryWaitKind::RateLimited);
    assert!((29..=30).contains(&secs), "got {secs}");
    // Another conversation is unaffected.
    assert_eq!(current_retry_wait(AIConversationId::new()), None);
    assert!(clear_retry_wait(id));
    assert!(!clear_retry_wait(id));
    assert_eq!(current_retry_wait(id), None);
}

#[test]
fn retry_wait_expires_on_its_own() {
    let id = AIConversationId::new();
    set_retry_wait(id, RetryWaitKind::Connection, Duration::ZERO);
    assert_eq!(current_retry_wait(id), None);
    clear_retry_wait(id);
}

#[test]
fn retry_wait_kind_follows_the_error() {
    let rl = AIApiError::ErrorStatus(http::StatusCode::TOO_MANY_REQUESTS, String::new(), None);
    let hinted = AIApiError::ErrorStatus(
        http::StatusCode::SERVICE_UNAVAILABLE,
        String::new(),
        Some(std::time::Duration::from_secs(5)),
    );
    let bad_gateway = AIApiError::ErrorStatus(http::StatusCode::BAD_GATEWAY, String::new(), None);
    assert_eq!(RetryWaitKind::for_error(&rl), RetryWaitKind::RateLimited);
    assert_eq!(
        RetryWaitKind::for_error(&hinted),
        RetryWaitKind::RateLimited
    );
    assert_eq!(
        RetryWaitKind::for_error(&bad_gateway),
        RetryWaitKind::Connection
    );
}

/// #903: the countdown must keep ticking once a second for the whole wait, independent of any
/// response stream (a resume waits after its stream is gone). Each tick nudges observers of the
/// history model, which is what makes the status bar recompute "retrying in N s".
#[test]
fn retry_wait_pulse_keeps_ticking_until_the_wait_ends() {
    use crate::ai::blocklist::history_model::BlocklistAIHistoryModel;
    use warpui::{App, Entity, ModelContext};

    struct TickCounter(usize);
    impl Entity for TickCounter {
        type Event = ();
    }

    App::test((), |mut app| async move {
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let counter = app.add_model(|ctx: &mut ModelContext<TickCounter>| {
            ctx.observe(&history, |me, _, _| me.0 += 1);
            TickCounter(0)
        });
        let id = AIConversationId::new();
        set_retry_wait(id, RetryWaitKind::RateLimited, Duration::from_millis(2500));
        history.update(&mut app, |_, ctx| super::start_retry_wait_pulse(id, ctx));
        warpui::r#async::Timer::after(Duration::from_millis(2300)).await;
        let ticks = counter.read(&app, |c, _| c.0);
        assert!(ticks >= 3, "countdown pulsed only {ticks} times in 2.3s");
        // A second start while the chain runs must not double it up.
        history.update(&mut app, |_, ctx| super::start_retry_wait_pulse(id, ctx));
        clear_retry_wait(id);
    });
}

/// #1381 (APP-R8): ending the wait releases the conversation's pulse registration at once. The
/// chain's next tick runs on a spawn, and a spawn on a dropped history model never fires, so
/// relying on that tick to deregister left the id registered forever and no later wait for the
/// conversation could pulse again.
#[test]
fn clearing_the_wait_releases_the_pulse_registration() {
    use super::retry_wait_pulse_running;
    use crate::ai::blocklist::history_model::BlocklistAIHistoryModel;
    use warpui::App;

    App::test((), |mut app| async move {
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let id = AIConversationId::new();
        set_retry_wait(id, RetryWaitKind::Connection, Duration::from_secs(30));
        history.update(&mut app, |_, ctx| super::start_retry_wait_pulse(id, ctx));
        assert!(retry_wait_pulse_running(id));
        // The stream is cancelled: no tick has to run for the registration to go away.
        assert!(clear_retry_wait(id));
        assert!(!retry_wait_pulse_running(id));
        // A later wait starts its own chain.
        set_retry_wait(id, RetryWaitKind::Connection, Duration::from_secs(30));
        history.update(&mut app, |_, ctx| super::start_retry_wait_pulse(id, ctx));
        assert!(retry_wait_pulse_running(id));
        clear_retry_wait(id);
    });
}

// ── a request waits for the keyring read instead of sending an empty key (#1368) ──

mod key_wait {
    use std::collections::HashMap;

    use futures::StreamExt as _;
    use futures::channel::oneshot;

    use super::super::{KeyWait, byop_keyring_error_response_stream, wait_for_loaded_keys};
    use crate::ai::agent_providers::provider_may_send_api_key;
    use crate::settings::{AgentProvider, AgentProviderApiType};

    fn run<F: std::future::Future>(f: F) -> F::Output {
        warpui::r#async::block_on(f)
    }

    #[test]
    fn a_request_sent_before_the_load_waits_and_then_carries_the_real_key() {
        let (load_tx, load_rx) = oneshot::channel();
        let (_cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
        let mut api_key = String::new();

        // Nothing resolved yet: the wait is pending, not an empty-key send.
        let mut wait = Box::pin(wait_for_loaded_keys(
            load_rx,
            &mut cancel_rx,
            "prov",
            &mut api_key,
        ));
        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        assert!(std::future::Future::poll(wait.as_mut(), &mut cx).is_pending());

        load_tx
            .send(Some(HashMap::from([(
                "prov".to_owned(),
                "sk-real".to_owned(),
            )])))
            .unwrap();
        assert!(matches!(run(wait), KeyWait::Ready));
        assert_eq!(api_key, "sk-real");
    }

    #[test]
    fn a_failed_load_fails_the_request_with_a_clear_error_and_no_key() {
        let (load_tx, load_rx) = oneshot::channel();
        let (_cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
        let mut api_key = String::new();
        load_tx.send(None).unwrap();
        assert!(matches!(
            run(wait_for_loaded_keys(
                load_rx,
                &mut cancel_rx,
                "prov",
                &mut api_key
            )),
            KeyWait::Failed
        ));
        assert!(api_key.is_empty());

        // The error the user sees is the keyring message, not an auth failure from a provider.
        let (_tx, rx) = oneshot::channel::<()>();
        let stream = run(byop_keyring_error_response_stream(rx)).unwrap();
        let items: Vec<_> = run(stream.collect::<Vec<_>>());
        assert_eq!(items.len(), 1);
        let err = match items.into_iter().next().unwrap() {
            Err(err) => err,
            Ok(_) => panic!("expected the keyring error, not a response event"),
        };
        assert!(
            err.to_string().contains("API key from the keyring"),
            "{err}"
        );
        // Not retried: the user has to unlock the keyring.
        assert!(!err.is_retryable());
    }

    #[test]
    fn a_cancel_while_waiting_sends_nothing() {
        let (_load_tx, load_rx) = oneshot::channel();
        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
        let mut api_key = String::new();
        cancel_tx.send(()).unwrap();
        assert!(matches!(
            run(wait_for_loaded_keys(
                load_rx,
                &mut cancel_rx,
                "prov",
                &mut api_key
            )),
            KeyWait::Cancelled
        ));
    }

    fn provider(api_type: AgentProviderApiType, base_url: &str) -> AgentProvider {
        AgentProvider {
            id: "p".to_owned(),
            name: "P".to_owned(),
            kind: Default::default(),
            api_type,
            base_url: base_url.to_owned(),
            models: Vec::new(),
            extra_headers: Vec::new(),
            vertex_project: String::new(),
            vertex_location: String::new(),
            disabled: false,
            token_price: None,
        }
    }

    #[test]
    fn keyless_providers_never_wait_for_the_keyring() {
        // A LAN Ollama (documentation address): no key is ever sent, so a slow keyring must not delay it.
        assert!(!provider_may_send_api_key(&provider(
            AgentProviderApiType::Ollama,
            "http://198.51.100.7:11434"
        )));
        // Plaintext non-loopback http: the key is withheld anyway.
        assert!(!provider_may_send_api_key(&provider(
            AgentProviderApiType::OpenAi,
            "http://198.51.100.7:8080/v1"
        )));
        // Hosted APIs and loopback gateways can carry a key and do wait.
        assert!(provider_may_send_api_key(&provider(
            AgentProviderApiType::Anthropic,
            "https://api.anthropic.com/v1"
        )));
        assert!(provider_may_send_api_key(&provider(
            AgentProviderApiType::OpenAi,
            "http://127.0.0.1:8787/v1"
        )));
    }
}
