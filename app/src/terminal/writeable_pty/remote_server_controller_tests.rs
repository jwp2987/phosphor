use super::{
    connection_label_from_session_hosts, connection_label_from_ssh_host,
    connection_label_from_user_and_host, early_bootstrap_flush_from_env,
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
