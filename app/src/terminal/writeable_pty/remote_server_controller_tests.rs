use super::{
    KnownExtensionHosts, connection_label_from_session_hosts, connection_label_from_ssh_host,
    connection_label_from_user_and_host, early_bootstrap_flush_from_env,
    early_flush_known_hosts_from_env,
};

#[test]
fn connection_label_prefers_ssh_host_over_reported_hostname() {
    assert_eq!(
        connection_label_from_session_hosts(
            "moira",
            "remote-reported-hostname",
            Some("ssh-user@devbox.namespace"),
        ),
        "moira@devbox.namespace"
    );
    assert_eq!(
        connection_label_from_session_hosts("moira", "remote-reported-hostname", None),
        "moira@remote-reported-hostname"
    );
}

#[test]
fn connection_label_from_ssh_host_strips_user_prefix() {
    assert_eq!(
        connection_label_from_ssh_host("moira@moira.devbox.namespace"),
        "moira.devbox.namespace"
    );
    assert_eq!(
        connection_label_from_ssh_host("moira.devbox.namespace"),
        "moira.devbox.namespace"
    );
}

#[test]
fn connection_label_from_user_and_host_matches_udi_format() {
    assert_eq!(
        connection_label_from_user_and_host("kevinyang", Some("ssh-testing")),
        "kevinyang@ssh-testing"
    );
    assert_eq!(
        connection_label_from_user_and_host("kevinyang", None),
        "kevinyang"
    );
    assert_eq!(
        connection_label_from_user_and_host("", Some("ssh-testing")),
        "ssh-testing"
    );
    assert_eq!(connection_label_from_user_and_host("", None), "Remote host");
}

#[test]
fn early_bootstrap_flush_is_on_unless_switched_off() {
    // #1172: default on; only an explicit off value restores the old order.
    assert!(early_bootstrap_flush_from_env(None));
    assert!(early_bootstrap_flush_from_env(Some("")));
    assert!(early_bootstrap_flush_from_env(Some("1")));
    assert!(early_bootstrap_flush_from_env(Some("on")));
    assert!(!early_bootstrap_flush_from_env(Some("0")));
    assert!(!early_bootstrap_flush_from_env(Some(" 0 ")));
    assert!(!early_bootstrap_flush_from_env(Some("false")));
    assert!(!early_bootstrap_flush_from_env(Some("off")));
    assert!(!early_bootstrap_flush_from_env(Some("no")));
}

/// #1266: a passed binary check waits for a speculative connect that is still in flight instead
/// of racing it with a second connect; once it failed, or when it never ran, the probe-driven
/// connect starts as before.
#[test]
fn a_passed_check_waits_for_an_in_flight_speculative_connect() {
    use super::{CheckPassedAction, SpeculativeConnect, check_passed_action};

    assert_eq!(
        check_passed_action(SpeculativeConnect::InFlight),
        CheckPassedAction::WaitForSpeculativeConnect
    );
    assert_eq!(
        check_passed_action(SpeculativeConnect::Failed),
        CheckPassedAction::StartConnect
    );
    assert_eq!(
        check_passed_action(SpeculativeConnect::Off),
        CheckPassedAction::StartConnect
    );
}

#[test]
fn speculative_connect_is_on_unless_switched_off() {
    use super::speculative_connect_from_env;

    assert!(speculative_connect_from_env(None));
    assert!(speculative_connect_from_env(Some("")));
    assert!(speculative_connect_from_env(Some("1")));
    assert!(!speculative_connect_from_env(Some("0")));
    assert!(!speculative_connect_from_env(Some(" off ")));
    assert!(!speculative_connect_from_env(Some("false")));
}

#[test]
fn early_flush_of_known_hosts_is_on_unless_switched_off() {
    // #1172: kill switch PHOSPHOR_SSH_EARLY_FLUSH_KNOWN=0.
    assert!(early_flush_known_hosts_from_env(None));
    assert!(early_flush_known_hosts_from_env(Some("1")));
    assert!(!early_flush_known_hosts_from_env(Some("0")));
    assert!(!early_flush_known_hosts_from_env(Some(" off ")));
    assert!(!early_flush_known_hosts_from_env(Some("false")));
}

#[test]
fn known_extension_hosts_remember_and_forget() {
    // A host is early-flushed only after a pass; any non-pass verdict removes it again so the
    // next connect goes back through the stashed install/prompt flow.
    let hosts = KnownExtensionHosts::default();
    assert!(!hosts.contains("me@box"));
    hosts.remember("me@box");
    assert!(hosts.contains("me@box"));
    assert!(!hosts.contains("me@other"));
    hosts.forget("me@box");
    assert!(!hosts.contains("me@box"));
}
