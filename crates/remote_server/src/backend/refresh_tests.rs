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
