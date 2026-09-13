use super::*;
use remote_server::setup::{RemoteArch, RemoteOs};
use warpui::{App, Entity, SingletonEntity};

/// A stand-in for whatever model actually calls the registry-observation helpers in
/// production (`RemoteServerController`'s own `ModelContext<M>`).
///
/// These helpers are generic over `M` precisely so they can be driven without building a
/// full `RemoteServerController`, but `M` must not be `HostRegistryModel` itself: each one
/// does `HostRegistryModel::handle(ctx).update(ctx, ..)` internally, so entering from that
/// model's own context re-enters a checked-out entity and panics with "Circular model
/// update" (`warpui_core`'s `update_model`). Production can never do that -- the caller is
/// always some other model -- so driving these from an unrelated model is what actually
/// reproduces the production call shape.
struct RegistryObservationCaller;

impl Entity for RegistryObservationCaller {
    type Event = ();
}

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

// --- Host registry wiring (`docs/design/moth-parliament.md`, "Requirement 5
// needs a surface, and a registry that does not exist") ---------------------
//
// These drive `record_binary_check_registry_observations` /
// `record_install_complete_registry_observation` directly rather than
// through a full `RemoteServerController` (which needs a `PtyController` +
// `ModelEventDispatcher` harness to construct) or a real
// `RemoteServerManagerEvent` dispatch -- both are generic over the caller's
// model type for exactly this reason (see their doc comments), so a
// `HostRegistryModel` update closure supplies a perfectly good
// `ModelContext` on its own.

/// Breaks if the call to `record_binary_check_registry_observations` were
/// deleted from `on_binary_check_complete`, or if that function stopped
/// calling `HostRegistryModel::record_reached` (e.g. only recording on the
/// unsupported branch).
#[test]
fn binary_check_probe_records_platform_as_reached() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(HostRegistryModel::new);

        let platform = RemotePlatform {
            os: RemoteOs::Linux,
            arch: RemoteArch::X86_64,
        };

        let caller = app.add_model(|_| RegistryObservationCaller);
        caller.update(&mut app, |_caller, ctx| {
            record_binary_check_registry_observations("build-box", Some(&platform), None, ctx);
        });

        app.read(|ctx| {
            let entry = HostRegistryModel::as_ref(ctx)
                .host("build-box")
                .expect("a probed host must appear in the registry");
            assert_eq!(entry.os, Some(RemoteOs::Linux));
            assert_eq!(entry.arch, Some(RemoteArch::X86_64));
            assert!(
                entry.last_reached_at.is_some(),
                "answering a probe must stamp last_reached_at"
            );
            assert_eq!(
                entry.install_state,
                HostInstallState::Unknown,
                "a Supported/Unknown probe carries no install-state information \
                 and must not touch it"
            );
        });
    });
}

/// Breaks if `record_binary_check_registry_observations` stopped recording
/// the unsupported reason (e.g. if the `if let Some(reason) =
/// unsupported_reason` branch were removed, or if it recorded
/// `HostInstallState::Unknown`/`NotInstalled` instead of `Unsupported` --
/// which is exactly the bug this requirement exists to avoid: an unsupported
/// host must be visibly distinct from one that was simply never probed).
#[test]
fn unsupported_preinstall_probe_records_reason_not_unknown() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(HostRegistryModel::new);

        let reason = UnsupportedReason::NonGlibc {
            name: "musl".to_string(),
        };

        let caller = app.add_model(|_| RegistryObservationCaller);
        caller.update(&mut app, |_caller, ctx| {
            record_binary_check_registry_observations("build-box", None, Some(&reason), ctx);
        });

        app.read(|ctx| {
            let entry = HostRegistryModel::as_ref(ctx)
                .host("build-box")
                .expect("a probed host must appear in the registry");
            assert_eq!(
                entry.install_state,
                HostInstallState::Unsupported {
                    reason: reason.clone()
                }
            );
        });
    });
}

/// Breaks if `record_install_complete_registry_observation` were removed
/// from the `Ok(())` arm of `on_binary_install_complete`, or if it recorded
/// anything other than `Installed` (e.g. left the prior state untouched, so
/// a fresh install never showed up as installed at all).
#[test]
fn install_complete_records_installed_with_no_known_version() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(HostRegistryModel::new);

        let caller = app.add_model(|_| RegistryObservationCaller);
        caller.update(&mut app, |_caller, ctx| {
            record_install_complete_registry_observation("build-box", ctx);
        });

        app.read(|ctx| {
            let entry = HostRegistryModel::as_ref(ctx)
                .host("build-box")
                .expect("a freshly-installed host must appear in the registry");
            assert_eq!(
                entry.install_state,
                HostInstallState::Installed { version: None },
                "installed-but-unknown-version must be recorded as Installed with \
                 version: None, not left as NotInstalled/Unknown and not guessed"
            );
        });
    });
}

/// Breaks if `registry_target_for_session_info` started reading the wrong
/// field (e.g. the reported hostname, or deriving from `session_info.user`
/// like the display label does, instead of the parsed `ssh` command's
/// destination), which would make the registry's `target` disagree with
/// `warpify.ssh.remote_hosts`. The local shell user ("localuser") is
/// deliberately different from the `ssh`-embedded user ("moira") so the two
/// functions' outputs actually diverge here, rather than coincidentally
/// matching.
#[test]
fn registry_target_reads_the_parsed_ssh_destination_not_the_display_label() {
    let mut session_info = crate::terminal::model::session::SessionInfo::new_for_test()
        .with_user("localuser".to_string())
        .with_hostname("remote-reported-hostname".to_string());
    session_info.subshell_info = Some(
        crate::terminal::model::terminal_model::SubshellInitializationInfo {
            spawning_command: "ssh moira@build-box".to_string(),
            was_triggered_by_rc_file_snippet: false,
            env_var_collection_name: None,
            ssh_connection_info: Some(crate::terminal::ssh::util::InteractiveSshCommand {
                host: Some("moira@build-box".to_string()),
                port: None,
            }),
        },
    );

    // The registry target is the raw parsed destination, unchanged...
    assert_eq!(
        registry_target_for_session_info(&session_info).as_deref(),
        Some("moira@build-box")
    );
    // ...which is deliberately not what the display label shows: that one
    // strips the ssh-embedded user and substitutes the local shell user.
    assert_eq!(
        connection_label_for_session_info(&session_info),
        "localuser@build-box"
    );
}
