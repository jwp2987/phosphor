use super::*;
use remote_server::setup::{GlibcVersion, UnsupportedReason};

fn host(target: &str, install_state: HostInstallState) -> RemoteHostEntry {
    RemoteHostEntry {
        target: target.to_string(),
        host_id: None,
        install_state,
        last_reached_at: None,
        os: None,
        arch: None,
    }
}

/// The central claim of this pane: a host that has never been probed reads as "Never reached",
/// never as "Not installed". Fails if `host_install_display` ever maps
/// `HostInstallState::Unknown` to `HostInstallDisplay::NotInstalled` (or to anything sharing
/// `NotInstalled`'s label) -- collapsing the two would render an unprobed host as though it had
/// been checked and found missing the remote server. See the module doc comment.
#[test]
fn never_reached_is_distinct_from_not_installed() {
    let never_reached = host_install_display(&HostInstallState::Unknown);
    let not_installed = host_install_display(&HostInstallState::NotInstalled);

    assert_eq!(never_reached, HostInstallDisplay::NeverReached);
    assert_eq!(not_installed, HostInstallDisplay::NotInstalled);
    assert_ne!(
        never_reached, not_installed,
        "an unprobed host and a host confirmed missing the remote server must not collapse \
         into the same display state"
    );
    assert_ne!(
        host_install_label(&never_reached),
        host_install_label(&not_installed),
        "the two states must render different text, not just different enum variants"
    );
}

#[test]
fn installed_state_carries_its_version_into_the_label() {
    let display = host_install_display(&HostInstallState::Installed {
        version: Some("1.2.3".to_string()),
    });
    assert_eq!(
        display,
        HostInstallDisplay::Installed {
            version: Some("1.2.3".to_string())
        }
    );
    assert!(host_install_label(&display).contains("1.2.3"));
}

/// `HostInstallState::Installed { version: None }` is a real value: install can complete
/// before any handshake has reported a real `InitializeResponse::server_version` -- see the
/// doc comment on `host_install_label`. Fails if a missing version is ever rendered as a
/// version (e.g. "Installed (v)") instead of its own "unknown" label, and fails if that label
/// is ever confused with `NotInstalled`'s or `NeverReached`'s.
#[test]
fn installed_with_no_version_reads_as_unknown_not_as_a_blank_version() {
    let display = host_install_display(&HostInstallState::Installed { version: None });
    let label = host_install_label(&display);

    assert!(
        !label.contains("(v)"),
        "a missing version must never render as though it were a real version: got {label:?}"
    );
    assert_ne!(label, host_install_label(&HostInstallDisplay::NotInstalled));
    assert_ne!(label, host_install_label(&HostInstallDisplay::NeverReached));
}

#[test]
fn unsupported_state_names_the_reason() {
    let display = host_install_display(&HostInstallState::Unsupported {
        reason: UnsupportedReason::GlibcTooOld {
            detected: GlibcVersion::new(2, 17),
            required: GlibcVersion::new(2, 28),
        },
    });
    let label = host_install_label(&display);
    assert!(label.contains("2.17"));
    assert!(label.contains("2.28"));

    let non_glibc = host_install_display(&HostInstallState::Unsupported {
        reason: UnsupportedReason::NonGlibc {
            name: "musl".to_string(),
        },
    });
    assert!(host_install_label(&non_glibc).contains("musl"));
}

/// `last_reached_label` draws the same never-vs-observed distinction as install state, over the
/// registry's other advisory field. Fails if `None` and `Some(_)` ever produce the same text.
#[test]
fn last_reached_label_distinguishes_never_from_a_real_timestamp() {
    let never = last_reached_label(None);
    let observed = last_reached_label(Some(Utc::now()));
    assert_eq!(never, "Never");
    assert_ne!(never, observed);
}

#[test]
fn platform_label_reports_unknown_only_when_nothing_is_observed() {
    assert_eq!(platform_label(None, None), "Unknown");
    assert_eq!(
        platform_label(Some("linux"), Some("x86_64")),
        "linux / x86_64"
    );
    assert_eq!(platform_label(Some("linux"), None), "linux");
    assert_eq!(platform_label(None, Some("x86_64")), "x86_64");
}

/// `host_row_text` is what the pane actually renders per host; this locks in that a
/// never-reached host's row never contains the word "installed" on its own (it says "Never
/// reached", not some substring collision), while a positively-installed host's row does.
#[test]
fn host_row_text_reflects_install_state_distinctly() {
    let never_reached = host("build-box", HostInstallState::Unknown);
    let not_installed = host("build-box", HostInstallState::NotInstalled);
    let installed = host(
        "build-box",
        HostInstallState::Installed {
            version: Some("9.9.9".to_string()),
        },
    );

    assert!(host_row_text(&never_reached).contains("Never reached"));
    assert!(!host_row_text(&never_reached).contains("Not installed"));
    assert!(host_row_text(&not_installed).contains("Not installed"));
    assert!(host_row_text(&installed).contains("Installed (v9.9.9)"));
}

#[test]
fn group_row_text_lists_members_or_says_so_when_empty() {
    let empty = HostGroup {
        name: "prod-api".to_string(),
        members: vec![],
    };
    let populated = HostGroup {
        name: "prod-api".to_string(),
        members: vec!["web-1".to_string(), "web-2".to_string()],
    };

    assert!(group_row_text(&empty).contains("no members"));
    let populated_text = group_row_text(&populated);
    assert!(populated_text.contains("web-1"));
    assert!(populated_text.contains("web-2"));
}
