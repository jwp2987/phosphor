use super::*;

const ALL_GLOBAL: [GlobalInstallMode; 3] = [
    GlobalInstallMode::AlwaysAsk,
    GlobalInstallMode::AlwaysInstall,
    GlobalInstallMode::NeverInstall,
];

#[test]
fn the_default_policy_is_ask_and_follows_the_app_wide_mode() {
    assert_eq!(HelperPolicy::default(), HelperPolicy::Ask);
    assert_eq!(
        decide(HelperPolicy::Ask, GlobalInstallMode::AlwaysAsk),
        HelperDecision::Ask
    );
    assert_eq!(
        decide(HelperPolicy::Ask, GlobalInstallMode::AlwaysInstall),
        HelperDecision::Install
    );
    assert_eq!(
        decide(HelperPolicy::Ask, GlobalInstallMode::NeverInstall),
        HelperDecision::Skip
    );
}

#[test]
fn a_hosts_own_decision_beats_the_app_wide_mode() {
    for global in ALL_GLOBAL {
        assert_eq!(decide(HelperPolicy::Allow, global), HelperDecision::Install);
        assert_eq!(decide(HelperPolicy::Never, global), HelperDecision::Skip);
    }
}

#[test]
fn only_never_stops_an_existing_helper_being_updated() {
    assert!(may_update_existing(HelperPolicy::Ask));
    assert!(may_update_existing(HelperPolicy::Allow));
    assert!(!may_update_existing(HelperPolicy::Never));
}

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn policy_is_read_from_the_two_lists_and_never_wins_a_conflict() {
    let allowed = list(&["a@x", "both@x"]);
    let denied = list(&["b@x", "both@x"]);
    assert_eq!(
        policy_from_lists("a@x", &allowed, &denied),
        HelperPolicy::Allow
    );
    assert_eq!(
        policy_from_lists("b@x", &allowed, &denied),
        HelperPolicy::Never
    );
    assert_eq!(
        policy_from_lists("both@x", &allowed, &denied),
        HelperPolicy::Never
    );
    assert_eq!(
        policy_from_lists("c@x", &allowed, &denied),
        HelperPolicy::Ask
    );
}

#[test]
fn setting_a_policy_moves_the_host_between_lists_without_duplicates() {
    let mut allowed = list(&["h"]);
    let mut denied = Vec::new();
    set_policy("h", HelperPolicy::Never, &mut allowed, &mut denied);
    assert!(allowed.is_empty());
    assert_eq!(denied, list(&["h"]));
    set_policy("h", HelperPolicy::Never, &mut allowed, &mut denied);
    assert_eq!(denied, list(&["h"]), "setting twice must not duplicate");
    set_policy("h", HelperPolicy::Allow, &mut allowed, &mut denied);
    assert_eq!((allowed, denied.is_empty()), (list(&["h"]), true));
    let mut allowed = list(&["h", "other"]);
    set_policy("h", HelperPolicy::Ask, &mut allowed, &mut denied);
    assert_eq!(allowed, list(&["other"]));
    assert!(denied.is_empty());
}

/// #1546 (upgrade from v0.1.17): in v0.1.17 "don't ask me again" + Install wrote the app-wide mode `always_install`, and there
/// were no host lists. After the upgrade a host in neither list still follows that choice, and the first per-host decision overrides it.
#[test]
fn a_v0_1_17_profile_keeps_its_app_wide_choice_until_a_host_decides() {
    let none: Vec<String> = Vec::new();
    for host in ["a@one", "b@two"] {
        let policy = policy_from_lists(host, &none, &none);
        assert_eq!(policy, HelperPolicy::Ask);
        assert_eq!(
            decide(policy, GlobalInstallMode::AlwaysInstall),
            HelperDecision::Install
        );
        assert_eq!(
            decide(policy, GlobalInstallMode::NeverInstall),
            HelperDecision::Skip
        );
        assert_eq!(
            decide(policy, GlobalInstallMode::AlwaysAsk),
            HelperDecision::Ask
        );
    }
    // one host says never: it stops installing and updating, the other host still follows the old choice
    let denied = list(&["b@two"]);
    let b = policy_from_lists("b@two", &none, &denied);
    assert_eq!(
        decide(b, GlobalInstallMode::AlwaysInstall),
        HelperDecision::Skip
    );
    assert!(!may_update_existing(b));
    let a = policy_from_lists("a@one", &none, &denied);
    assert_eq!(
        decide(a, GlobalInstallMode::AlwaysInstall),
        HelperDecision::Install
    );
}
