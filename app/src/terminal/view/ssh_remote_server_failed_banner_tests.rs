//! Unit tests for the pure decision logic in this module: which text is
//! shown for which state. Deliberately does not construct
//! `SshRemoteServerFailedBanner` itself (a `View`, needing a `ViewContext`)
//! -- these tests pin the model-level mapping from state to message, per
//! TODO.md's "Remote-session setup degrades silently" entry.

use remote_server::setup::GlibcVersion;

use super::*;

fn init_i18n() {
    crate::i18n::init(Some("en"));
}

#[test]
fn unsupported_kind_has_distinct_title_and_description_from_the_failure_kinds() {
    init_i18n();

    let unsupported_title = SshRemoteServerFailureKind::Unsupported.title();
    let unsupported_description = SshRemoteServerFailureKind::Unsupported.description();

    for failure_kind in [
        SshRemoteServerFailureKind::BinaryCheck,
        SshRemoteServerFailureKind::BinaryInstall,
        SshRemoteServerFailureKind::Launch,
    ] {
        assert_ne!(
            failure_kind.title(),
            unsupported_title,
            "Unsupported is not a failure and must read differently from {failure_kind:?}",
        );
        assert_ne!(
            failure_kind.description(),
            unsupported_description,
            "Unsupported is not a failure and must read differently from {failure_kind:?}",
        );
    }

    // Not just non-empty -- must actually have resolved through fluent
    // rather than silently falling back to returning the raw message id
    // (which `crate::t!` does when the loader isn't initialized; see
    // `app/src/terminal/host_footer_color_tests.rs` for the same guard).
    assert_ne!(
        unsupported_title,
        "terminal-ssh-remote-server-unsupported-title"
    );
    assert_ne!(
        unsupported_description,
        "terminal-ssh-remote-server-unsupported-description"
    );
}

#[test]
fn glibc_too_old_names_both_versions() {
    init_i18n();

    let detail = describe_unsupported_reason(&UnsupportedReason::GlibcTooOld {
        detected: GlibcVersion::new(2, 17),
        required: GlibcVersion::new(2, 31),
    });

    assert!(
        detail.contains("2.17"),
        "must name the detected version, got: {detail}"
    );
    assert!(
        detail.contains("2.31"),
        "must name the required version, got: {detail}"
    );
}

#[test]
fn non_glibc_names_the_detected_libc() {
    init_i18n();

    let detail = describe_unsupported_reason(&UnsupportedReason::NonGlibc {
        name: "musl".to_string(),
    });

    assert!(
        detail.contains("musl"),
        "must name the detected libc, got: {detail}"
    );
}

#[test]
fn the_two_unsupported_reasons_produce_different_text() {
    init_i18n();

    let glibc_detail = describe_unsupported_reason(&UnsupportedReason::GlibcTooOld {
        detected: GlibcVersion::new(2, 17),
        required: GlibcVersion::new(2, 31),
    });
    let non_glibc_detail = describe_unsupported_reason(&UnsupportedReason::NonGlibc {
        name: "musl".to_string(),
    });

    assert_ne!(glibc_detail, non_glibc_detail);
}

#[test]
fn legacy_kind_has_distinct_title_and_description_from_every_other_kind() {
    init_i18n();

    let legacy_title = SshRemoteServerFailureKind::Legacy.title();
    let legacy_description = SshRemoteServerFailureKind::Legacy.description();

    for other_kind in [
        SshRemoteServerFailureKind::BinaryCheck,
        SshRemoteServerFailureKind::BinaryInstall,
        SshRemoteServerFailureKind::Launch,
        SshRemoteServerFailureKind::Unsupported,
    ] {
        assert_ne!(
            other_kind.title(),
            legacy_title,
            "Legacy must read differently from {other_kind:?}",
        );
        assert_ne!(
            other_kind.description(),
            legacy_description,
            "Legacy must read differently from {other_kind:?}",
        );
    }

    // Same fluent-resolution guard as the Unsupported test above.
    assert_ne!(legacy_title, "terminal-ssh-remote-server-legacy-title");
    assert_ne!(
        legacy_description,
        "terminal-ssh-remote-server-legacy-description"
    );
}

#[test]
fn feature_disabled_and_no_connected_client_produce_different_text() {
    init_i18n();

    let feature_disabled =
        describe_legacy_fallback_reason(&LegacySshFallbackReason::FeatureDisabled);
    let no_client = describe_legacy_fallback_reason(&LegacySshFallbackReason::NoConnectedClient);

    assert_ne!(feature_disabled, no_client);
    // Same fluent-resolution guard as the Unsupported-reason tests above.
    assert_ne!(
        feature_disabled,
        "terminal-ssh-remote-server-legacy-feature-disabled-detail"
    );
    assert_ne!(
        no_client,
        "terminal-ssh-remote-server-legacy-no-client-detail"
    );
}
