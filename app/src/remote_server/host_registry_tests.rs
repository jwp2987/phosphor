//! New in this fork -- there is no pin equivalent to port these against.

use super::*;
use warpui::App;

// --- Pure convergence rule ---------------------------------------------

/// Breaks if `converge_declared_targets` stops inserting a stub entry for a
/// declared target it has not seen before, or stops being idempotent (e.g. if
/// the `contains_key` guard were removed and a second call overwrote an
/// already-observed host back to `Unknown`).
#[test]
fn declared_targets_gain_stub_entries_and_convergence_is_idempotent() {
    let mut hosts = HashMap::new();

    let added = converge_declared_targets(&mut hosts, &["build-box".to_string()]);
    assert_eq!(added, vec!["build-box".to_string()]);
    assert_eq!(hosts["build-box"].install_state, HostInstallState::Unknown);
    assert_eq!(hosts["build-box"].host_id, None);

    // Observing a real fact about the host...
    hosts.get_mut("build-box").unwrap().install_state = HostInstallState::Installed {
        version: Some("0.4.2".to_string()),
    };

    // ...must survive re-running convergence with the same declared list.
    let added_again = converge_declared_targets(&mut hosts, &["build-box".to_string()]);
    assert!(
        added_again.is_empty(),
        "an already-known target must not be reported as newly added"
    );
    assert_eq!(
        hosts["build-box"].install_state,
        HostInstallState::Installed {
            version: Some("0.4.2".to_string())
        },
        "re-converging must not clobber an observed install state back to Unknown"
    );

    // A second declared target is picked up independently.
    let added = converge_declared_targets(
        &mut hosts,
        &["build-box".to_string(), "gpu-box".to_string()],
    );
    assert_eq!(added, vec!["gpu-box".to_string()]);
    assert_eq!(hosts.len(), 2);
}

// --- Pure live-connection refresh rule -----------------------------------
//
// `refresh_from_live_connections` itself needs a real `RemoteServerManager` connection to
// exercise end to end, which requires constructing `RemoteSessionState::Connected` --
// private to (and only test-supported within) the `remote_server` crate's own
// `manager_tests.rs`, not reachable from this crate's tests. `live_targets` is the pure
// decision logic that function delegates to (which targets have a `host_id` currently
// connected), factored out for exactly this reason, matching
// `converge_declared_targets`'s precedent above.

/// Breaks if `live_targets` starts including a host with no `host_id` at all, a host whose
/// `host_id` is not in the connected set, or stops including one that is.
#[test]
fn live_targets_selects_only_hosts_with_a_connected_host_id() {
    let mut hosts = HashMap::new();

    let mut never_reached = RemoteHostEntry::new("never-reached");
    never_reached.host_id = None;
    hosts.insert(never_reached.target.clone(), never_reached);

    let mut connected = RemoteHostEntry::new("build-box");
    connected.host_id = Some(HostId::new("host-connected".to_string()));
    hosts.insert(connected.target.clone(), connected);

    let mut disconnected = RemoteHostEntry::new("gpu-box");
    disconnected.host_id = Some(HostId::new("host-disconnected".to_string()));
    hosts.insert(disconnected.target.clone(), disconnected);

    let connected_host_ids: HashSet<HostId> = [HostId::new("host-connected".to_string())]
        .into_iter()
        .collect();

    let mut selected = live_targets(&hosts, &connected_host_ids);
    selected.sort();
    assert_eq!(selected, vec!["build-box".to_string()]);
}

/// No connected hosts at all must select nothing, never panic or fall back to "refresh
/// everything".
#[test]
fn live_targets_is_empty_when_nothing_is_connected() {
    let mut hosts = HashMap::new();
    let mut entry = RemoteHostEntry::new("build-box");
    entry.host_id = Some(HostId::new("host-abc".to_string()));
    hosts.insert(entry.target.clone(), entry);

    assert!(live_targets(&hosts, &HashSet::new()).is_empty());
}

// --- RemoteHostEntry <-> PersistedRemoteHost conversion -----------------

fn base_entry() -> RemoteHostEntry {
    let mut entry = RemoteHostEntry::new("build-box");
    entry.host_id = Some(HostId::new("host-abc".to_string()));
    entry.last_reached_at = Some(Utc::now());
    entry.os = Some(RemoteOs::Linux);
    entry.arch = Some(RemoteArch::X86_64);
    entry
}

/// Breaks if `to_persisted`/`from_persisted` drop or mis-encode any of
/// `host_id`, `last_reached_at`, `os`/`arch`, or any `HostInstallState`
/// variant -- including the two `UnsupportedReason` shapes, which is exactly
/// the reuse the module doc calls out. This is "install state round-trips
/// through persistence" at the settings-value boundary: the actual settings
/// write/read round trip is covered by
/// `declared_remote_host_appears_in_the_registry` below, which goes through
/// `WarpifySettings` for real.
#[test]
fn install_state_round_trips_through_persisted_conversion_for_every_variant() {
    let cases = [
        HostInstallState::NotInstalled,
        HostInstallState::Installed {
            version: Some("0.4.2".to_string()),
        },
        // A completed install with no handshake yet to report a real version -- the case
        // `None` exists for, per the type's own doc comment. Round-tripping this proves the
        // "unknown version" state survives persistence as `None`, not as an empty string.
        HostInstallState::Installed { version: None },
        HostInstallState::Unsupported {
            reason: UnsupportedReason::GlibcTooOld {
                detected: GlibcVersion::new(2, 17),
                required: GlibcVersion::new(2, 31),
            },
        },
        HostInstallState::Unsupported {
            reason: UnsupportedReason::NonGlibc {
                name: "musl".to_string(),
            },
        },
        HostInstallState::Unknown,
    ];

    for install_state in cases {
        let mut entry = base_entry();
        entry.install_state = install_state.clone();

        let persisted = entry.to_persisted();
        let round_tripped = RemoteHostEntry::from_persisted(persisted);

        assert_eq!(
            round_tripped.install_state, install_state,
            "install state did not survive a to_persisted/from_persisted round trip"
        );
        assert_eq!(round_tripped.target, entry.target);
        assert_eq!(round_tripped.host_id, entry.host_id);
        assert_eq!(round_tripped.os, entry.os);
        assert_eq!(round_tripped.arch, entry.arch);
        // Timestamps go through millisecond-precision Unix time, so compare
        // at that precision rather than the original `DateTime<Utc>`, which
        // carries sub-millisecond precision `to_persisted` deliberately
        // discards (there is no sub-millisecond fact to preserve here).
        assert_eq!(
            round_tripped
                .last_reached_at
                .map(|ts| ts.timestamp_millis()),
            entry.last_reached_at.map(|ts| ts.timestamp_millis())
        );
    }
}

/// An `install_state` value this build does not recognize must fail open to
/// `Unknown` rather than panicking -- the same fail-open stance
/// `PreinstallCheckResult::parse` takes on data it cannot classify. This
/// matters more here than it would for a database column: a settings value
/// can arrive hand-edited or synced from a newer build.
#[test]
fn unrecognized_install_state_value_reads_back_as_unknown() {
    let mut persisted = base_entry().to_persisted();
    persisted.install_state = "some_future_state".to_string();
    assert_eq!(
        RemoteHostEntry::from_persisted(persisted).install_state,
        HostInstallState::Unknown
    );
}

// --- Model-level integration: settings convergence and group membership -

/// Requirement: "a host declared in the existing setting appears in the
/// registry". Breaks if `HostRegistryModel::new` stops converging
/// `WarpifySettings::remote_hosts` at construction, or if the
/// `WarpifySettingsChangedEvent::RemoteHosts` subscription is removed (the
/// second half of this test, adding `gpu-box` after construction). Also
/// covers the settings round trip for install state: `record_install_state`
/// below writes through `WarpifySettings::remote_host_registry_entries` and
/// the assertion reads it back through the same settings model, not through
/// `HostRegistryModel`'s own cache.
#[test]
fn declared_remote_host_appears_in_the_registry_and_install_state_round_trips() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);

        WarpifySettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .remote_hosts
                .set_value(vec!["build-box".to_string()], ctx)
                .expect("set_value should succeed for a plain string list");
        });

        app.add_singleton_model(HostRegistryModel::new);

        app.read(|ctx| {
            let registry = HostRegistryModel::as_ref(ctx);
            let entry = registry
                .host("build-box")
                .expect("a host declared in warpify.ssh.remote_hosts must appear in the registry");
            assert_eq!(entry.install_state, HostInstallState::Unknown);

            // The convergence stub must itself be visible through
            // `WarpifySettings`, not only through `HostRegistryModel`'s cache --
            // that is what makes this "persisted" rather than incidental.
            let persisted_targets: Vec<&str> = WarpifySettings::as_ref(ctx)
                .remote_host_registry_entries
                .value()
                .iter()
                .map(|entry| entry.target.as_str())
                .collect();
            assert!(persisted_targets.contains(&"build-box"));
        });

        // A target declared *after* the registry already exists must also
        // converge, not just the ones present at construction time.
        WarpifySettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .remote_hosts
                .set_value(vec!["build-box".to_string(), "gpu-box".to_string()], ctx)
                .expect("set_value should succeed for a plain string list");
        });

        app.read(|ctx| {
            assert!(
                HostRegistryModel::as_ref(ctx).host("gpu-box").is_some(),
                "a host added to the setting after registry construction must still converge"
            );
        });

        // Recording an install state must round-trip through
        // `WarpifySettings`: written by `HostRegistryModel`, read back
        // independently through the settings model.
        HostRegistryModel::handle(&app).update(&mut app, |model, ctx| {
            model.record_install_state(
                "build-box",
                HostInstallState::Installed {
                    version: Some("0.4.2".to_string()),
                },
                ctx,
            );
        });

        app.read(|ctx| {
            let persisted = WarpifySettings::as_ref(ctx)
                .remote_host_registry_entries
                .value()
                .iter()
                .find(|entry| entry.target == "build-box")
                .expect("build-box must still have a persisted entry")
                .clone();
            assert_eq!(persisted.install_state, "installed");
            assert_eq!(persisted.installed_version.as_deref(), Some("0.4.2"));

            assert_eq!(
                HostRegistryModel::as_ref(ctx)
                    .host("build-box")
                    .expect("host should still be present")
                    .install_state,
                HostInstallState::Installed {
                    version: Some("0.4.2".to_string())
                }
            );
        });
    });
}

/// Requirement: "group membership round-trips". Breaks if `add_host_to_group`
/// stops deduplicating, if `remove_host_from_group` removes the wrong member
/// or deletes the group itself, or if persistence through
/// `WarpifySettings::remote_host_groups` stops happening (the assertions read
/// the settings value directly, not just `HostRegistryModel`'s cache).
#[test]
fn group_membership_round_trips_through_settings() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);

        app.add_singleton_model(HostRegistryModel::new);

        HostRegistryModel::handle(&app).update(&mut app, |model, ctx| {
            model.add_host_to_group("prod-api", "web-1", ctx);
            model.add_host_to_group("prod-api", "web-2", ctx);
            // Re-adding an already-present member must not duplicate it.
            model.add_host_to_group("prod-api", "web-1", ctx);
        });

        app.read(|ctx| {
            let mut members = HostRegistryModel::as_ref(ctx)
                .group("prod-api")
                .expect("group should exist after adding members")
                .members
                .clone();
            members.sort();
            assert_eq!(members, vec!["web-1".to_string(), "web-2".to_string()]);

            // Persisted, not just cached: the settings model must agree.
            let persisted_groups = WarpifySettings::as_ref(ctx).remote_host_groups.value();
            assert_eq!(persisted_groups.len(), 1);
            let mut persisted_members = persisted_groups[0].members.clone();
            persisted_members.sort();
            assert_eq!(
                persisted_members,
                vec!["web-1".to_string(), "web-2".to_string()]
            );
        });

        HostRegistryModel::handle(&app).update(&mut app, |model, ctx| {
            model.remove_host_from_group("prod-api", "web-1", ctx);
        });

        app.read(|ctx| {
            let group = HostRegistryModel::as_ref(ctx)
                .group("prod-api")
                .expect("removing a member must not delete the group");
            assert_eq!(group.members, vec!["web-2".to_string()]);

            let persisted_groups = WarpifySettings::as_ref(ctx).remote_host_groups.value();
            assert_eq!(
                persisted_groups.len(),
                1,
                "the group row must survive its last-but-one member being removed"
            );
            assert_eq!(persisted_groups[0].members, vec!["web-2".to_string()]);
        });
    });
}
