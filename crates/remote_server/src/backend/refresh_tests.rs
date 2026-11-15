use super::*;

fn precmd(cwd: &str, cmd: Option<&str>) -> RemoteRefreshEvent {
    RemoteRefreshEvent::Precmd {
        cwd: cwd.into(),
        last_cmd: cmd.map(Into::into),
    }
}

#[test]
fn one_in_flight_and_a_dirty_flag_queue_exactly_one_more() {
    let mut c = RefreshCoordinator::new(PrecmdPolicy::EveryPrompt);
    assert_eq!(
        c.on_event(&RemoteRefreshEvent::UserAction),
        RefreshDecision::Start
    );
    assert!(c.in_flight());
    // Three events while running collapse into one rerun.
    for _ in 0..3 {
        assert_eq!(
            c.on_event(&RemoteRefreshEvent::AfterOwnWrite),
            RefreshDecision::Coalesced
        );
    }
    assert!(c.finished(), "a rerun is owed");
    assert!(c.in_flight());
    assert!(!c.finished(), "only one rerun, then idle");
    assert!(!c.in_flight());
    assert_eq!(
        c.on_event(&RemoteRefreshEvent::UserAction),
        RefreshDecision::Start
    );
}

#[test]
fn every_prompt_policy_refreshes_on_each_prompt_even_an_empty_enter() {
    let mut c = RefreshCoordinator::new(PrecmdPolicy::EveryPrompt);
    assert_eq!(c.on_event(&precmd("/r", Some(""))), RefreshDecision::Start);
    c.finished();
    assert_eq!(
        c.on_event(&precmd("/r", Some("sed -i x f"))),
        RefreshDecision::Start
    );
}

#[test]
fn git_relevant_policy_skips_unrelated_commands_in_the_same_cwd() {
    let mut c = RefreshCoordinator::new(PrecmdPolicy::GitRelevant);
    // First prompt: the cwd is new, so read once.
    assert_eq!(
        c.on_event(&precmd("/r", Some("ls"))),
        RefreshDecision::Start
    );
    c.finished();
    assert_eq!(
        c.on_event(&precmd("/r", Some("ls -la"))),
        RefreshDecision::Skip
    );
    assert_eq!(
        c.on_event(&precmd("/r", Some("cargo build"))),
        RefreshDecision::Skip
    );
    assert_eq!(c.on_event(&precmd("/r", Some(""))), RefreshDecision::Skip);
    assert_eq!(
        c.on_event(&precmd("/r", Some("git commit -m x"))),
        RefreshDecision::Start
    );
    c.finished();
    assert_eq!(
        c.on_event(&precmd("/r", Some("git checkout b"))),
        RefreshDecision::Start
    );
    c.finished();
    assert_eq!(
        c.on_event(&precmd("/r", Some("gh pr view"))),
        RefreshDecision::Start
    );
    c.finished();
    assert_eq!(
        c.on_event(&precmd("/r", Some("cd sub"))),
        RefreshDecision::Start
    );
}

#[test]
fn an_unknown_last_command_is_treated_as_relevant() {
    let mut c = RefreshCoordinator::new(PrecmdPolicy::GitRelevant);
    c.on_event(&precmd("/r", None));
    c.finished();
    assert_eq!(c.on_event(&precmd("/r", None)), RefreshDecision::Start);
}

#[test]
fn a_cwd_change_refreshes_once_per_new_directory() {
    let mut c = RefreshCoordinator::new(PrecmdPolicy::GitRelevant);
    assert_eq!(
        c.on_event(&RemoteRefreshEvent::CwdChanged { cwd: "/a".into() }),
        RefreshDecision::Start
    );
    c.finished();
    assert_eq!(
        c.on_event(&RemoteRefreshEvent::CwdChanged { cwd: "/a".into() }),
        RefreshDecision::Skip
    );
    assert_eq!(
        c.on_event(&RemoteRefreshEvent::CwdChanged { cwd: "/b".into() }),
        RefreshDecision::Start
    );
}

#[test]
fn a_skipped_event_while_running_does_not_set_dirty() {
    let mut c = RefreshCoordinator::new(PrecmdPolicy::GitRelevant);
    c.on_event(&precmd("/r", Some("ls")));
    assert_eq!(c.on_event(&precmd("/r", Some("ls"))), RefreshDecision::Skip);
    assert!(!c.finished());
}

#[test]
fn a_navigation_issued_before_the_first_execs_replaces_it_so_one_exec_runs_2229() {
    let mut tickets = NavigationTickets::new();
    let first = tickets.issue(1u32);
    let second = tickets.issue(1u32);
    // Each task checks its ticket right before it would exec: only the newest does.
    let execs = [first, second]
        .into_iter()
        .filter(|t| tickets.is_current(1, *t))
        .count();
    assert_eq!(execs, 1);
    assert!(tickets.is_current(1, second));
    assert!(!tickets.is_current(1, first));
}

#[test]
fn navigation_tickets_are_per_session_and_forgotten_on_reset_2229() {
    let mut tickets = NavigationTickets::new();
    let a = tickets.issue(1u32);
    let b = tickets.issue(2u32);
    assert!(tickets.is_current(1, a) && tickets.is_current(2, b));
    tickets.forget(1);
    assert!(!tickets.is_current(1, a));
    assert!(tickets.is_current(2, b));
}
