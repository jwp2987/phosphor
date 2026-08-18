use warp_core::features::FeatureFlag;
use warpui::{App, SingletonEntity};

use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
use crate::ai::execution_profiles::{
    AIExecutionProfile, AIExecutionProfileObject, AIExecutionProfileObjectModel, ActionPermission,
    ComputerUsePermission,
};
use crate::ai::mcp::TemplatableMCPServerManager;
use crate::auth::{AuthStateProvider, UserUid};
use crate::cloud_object::model::persistence::{ObjectStoreEvent, ObjectStoreModel};
use crate::cloud_object::update_manager::UpdateManager;
use crate::cloud_object::{Owner, StoredObjectMetadata, StoredObjectPermissions};
use crate::network::NetworkStatus;
use crate::server::ids::{ServerId, SyncId};
use crate::settings::PrivacySettings;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspaces::user_workspaces::UserWorkspaces;
use crate::LaunchMode;

/// Install the minimal singleton graph needed to construct an
/// `AIExecutionProfilesModel` and exercise its ObjectStoreModel interactions.
fn install_singletons(app: &mut App, auth_state: AuthStateProvider) {
    initialize_settings_for_tests(app);
    app.add_singleton_model(|_| auth_state);
    app.add_singleton_model(|_| NetworkStatus::new());
    app.add_singleton_model(UpdateManager::mock);
    app.add_singleton_model(ObjectStoreModel::mock);
    app.add_singleton_model(|_| TemplatableMCPServerManager::default());
    app.add_singleton_model(PrivacySettings::mock);
    app.add_singleton_model(UserWorkspaces::default_mock);
}

/// Regression test for the onboarding autonomy bug where
/// `edit_profile_internal` would silently drop edits made to an `Unsynced`
/// default profile whenever `personal_drive` returned `None` (logged-out
/// users). `apply_agent_settings` calls `set_*` on the default profile the
/// moment onboarding completes, which can happen before the user logs in
/// (e.g. `LoginSlideEvent::LoginLaterConfirmed`), so those edits must
/// persist on the local `Unsynced` state rather than being dropped.
#[test]
fn edits_persist_on_unsynced_default_profile_when_logged_out() {
    App::test((), |mut app| async move {
        install_singletons(&mut app, AuthStateProvider::new_logged_out_for_test());
        let profile_model = app.add_singleton_model(|ctx| {
            AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
        });

        let default_profile_id = profile_model.read(&app, |model, _ctx| model.default_profile_id());

        // Sanity-check the precondition: the baseline `apply_code_diffs`
        // on a fresh default profile is the enum default (`AgentDecides`).
        profile_model.read(&app, |model, ctx| {
            assert!(
                matches!(
                    model.default_profile(ctx).data().apply_code_diffs,
                    ActionPermission::AgentDecides
                ),
                "unexpected baseline apply_code_diffs"
            );
        });

        // Apply the edit that onboarding would make for the Full autonomy
        // preset. Before the fix, this call no-ops because
        // `personal_drive` is `None` while the profile is `Unsynced` — the
        // `set_apply_code_diffs` value was cloned, mutated, then dropped
        // without being written back to `default_profile_state`.
        profile_model.update(&mut app, |model, ctx| {
            model.set_apply_code_diffs(default_profile_id, &ActionPermission::AlwaysAllow, ctx);
        });

        profile_model.read(&app, |model, ctx| {
            assert_eq!(
                model.default_profile(ctx).data().apply_code_diffs,
                ActionPermission::AlwaysAllow,
                "edit was dropped: default profile still has the baseline \
                 apply_code_diffs value after an edit made while logged out",
            );
        });
    })
}

/// Regression test for the "log in to an existing user after onboarding"
/// bug. Objects restored from local storage can already exist in `ObjectStoreModel`
/// before `AIExecutionProfilesModel` observes per-object `ObjectCreated` events.
/// The model reconciles when it receives `ObjectStoreEvent::InitialLoadCompleted`.
/// Without the reconciliation handler for `InitialLoadCompleted`, the
/// existing user's default profile sits in `ObjectStoreModel` but
/// `AIExecutionProfilesModel` stays in `Unsynced`, so a subsequent
/// onboarding edit creates a duplicate cloud default profile instead of
/// editing the existing one. This test drives that sequence and asserts
/// the model adopts the cloud profile's sync id.
#[test]
fn reconciles_unsynced_default_profile_with_cloud_after_initial_load() {
    App::test((), |mut app| async move {
        install_singletons(&mut app, AuthStateProvider::new_for_test());
        let profile_model = app.add_singleton_model(|ctx| {
            AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
        });

        // Baseline: ObjectStoreModel is empty, so the model starts Unsynced and
        // `sync_id` is `None`.
        profile_model.read(&app, |model, ctx| {
            assert!(
                model.default_profile(ctx).sync_id().is_none(),
                "default profile should be Unsynced at startup"
            );
        });

        // Simulate the user's existing default profile object arriving via
        // initial bulk load. We construct the existing profile with
        // `apply_code_diffs = AlwaysAllow` so we can verify the model is
        // reading that stored object after reconciliation.
        let cloud_uid = ServerId::from(42);
        let cloud_sync_id = SyncId::ServerId(cloud_uid);
        let local_profile = AIExecutionProfile {
            name: "Default".to_string(),
            is_default_profile: true,
            apply_code_diffs: ActionPermission::AlwaysAllow,
            ..Default::default()
        };
        let profile_object = AIExecutionProfileObject::new(
            cloud_sync_id,
            AIExecutionProfileObjectModel::new(local_profile),
            StoredObjectMetadata::mock(),
            StoredObjectPermissions::mock_personal(),
        );

        // Insert the object into ObjectStoreModel without per-object events and then
        // emit `InitialLoadCompleted` so the reconciliation handler fires.
        ObjectStoreModel::handle(&app).update(&mut app, move |object_store_model, ctx| {
            object_store_model.add_object(cloud_sync_id, profile_object);
            ctx.emit(ObjectStoreEvent::InitialLoadCompleted);
        });

        // The model should now be Synced with the stored profile object's sync_id,
        // and `default_profile` should read values from the existing local
        // object (proving we're not backed by a fresh client-side default).
        profile_model.read(&app, |model, ctx| {
            let info = model.default_profile(ctx);
            assert_eq!(
                info.sync_id(),
                Some(cloud_sync_id),
                "model did not adopt the existing default profile object's sync_id"
            );
            assert_eq!(
                info.data().apply_code_diffs,
                ActionPermission::AlwaysAllow,
                "default profile should now surface the existing stored value"
            );
        });

        // Further edits should now target the existing profile object in
        // place, rather than falling through the `Unsynced` branch and
        // creating a duplicate.
        let default_profile_id = profile_model.read(&app, |model, _ctx| model.default_profile_id());
        profile_model.update(&mut app, |model, ctx| {
            model.set_apply_code_diffs(default_profile_id, &ActionPermission::AlwaysAsk, ctx);
        });
        profile_model.read(&app, |model, ctx| {
            let info = model.default_profile(ctx);
            assert_eq!(
                info.sync_id(),
                Some(cloud_sync_id),
                "edit should target the same cloud sync_id, not create a duplicate"
            );
            assert_eq!(
                info.data().apply_code_diffs,
                ActionPermission::AlwaysAsk,
                "edit should be reflected on the existing profile object"
            );
        });
    })
}

/// Regression coverage for upstream
/// `c2954dcbc0` ("Prevent the client from reading non-personal AI execution
/// profiles", #25377, GHSA-cqw8-cqq2-8cjm) ported against this fork's own
/// `ObjectStoreModel`/`StoredObject` test scaffolding -- the pin's four tests
/// build `ServerAIExecutionProfile`/`ServerObjectGuest`/`AccessLevel` values
/// via `warp_graphql::object_permissions`, a crate this fork does not have
/// (cloud, dropped), so those exact tests are not portable verbatim. This is
/// a from-scratch equivalent of the pin's `ignores_shared_default_profile_after_initial_load`,
/// exercising the same `reconcile_with_cloud_state_after_initial_load` path as
/// the test above, but with an attacker-owned object that must NOT be adopted.
#[test]
fn ignores_non_owned_default_profile_after_initial_load() {
    App::test((), |mut app| async move {
        install_singletons(&mut app, AuthStateProvider::new_for_test());
        let profile_model = app.add_singleton_model(|ctx| {
            AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
        });

        let attacker_owner = Owner::User {
            user_uid: UserUid::new("attacker-owner"),
        };
        let attacker_sync_id = SyncId::ServerId(ServerId::from(31338));
        let attacker_profile = AIExecutionProfile {
            name: "Attacker Default".to_string(),
            is_default_profile: true,
            apply_code_diffs: ActionPermission::AlwaysAllow,
            ..Default::default()
        };
        let attacker_object = AIExecutionProfileObject::new(
            attacker_sync_id,
            AIExecutionProfileObjectModel::new(attacker_profile),
            StoredObjectMetadata::mock(),
            StoredObjectPermissions {
                owner: attacker_owner,
                permissions_last_updated_ts: None,
                anyone_with_link: None,
                guests: Vec::new(),
            },
        );

        ObjectStoreModel::handle(&app).update(&mut app, move |object_store_model, ctx| {
            object_store_model.add_object(attacker_sync_id, attacker_object);
            ctx.emit(ObjectStoreEvent::InitialLoadCompleted);
        });

        profile_model.read(&app, |model, ctx| {
            let default_profile = model.default_profile(ctx);
            assert_eq!(
                default_profile.sync_id(),
                None,
                "non-owned default profile should not be reconciled as default"
            );
            assert_eq!(
                default_profile.data().apply_code_diffs,
                ActionPermission::AgentDecides,
                "non-owned profile should not control diff-apply approvals"
            );
        });
    })
}

/// See
/// `ignores_non_owned_default_profile_after_initial_load` above for why this
/// is a from-scratch equivalent of the pin's coverage rather than a verbatim
/// port. Exercises the live-event path (`ObjectStoreModel::create_object`,
/// which emits `ObjectStoreEvent::ObjectCreated`) rather than the bulk
/// initial-load path, since upstream's fix touches both call sites.
#[test]
fn ignores_non_owned_default_profile_created_via_event() {
    App::test((), |mut app| async move {
        install_singletons(&mut app, AuthStateProvider::new_for_test());
        let profile_model = app.add_singleton_model(|ctx| {
            AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
        });

        let attacker_owner = Owner::User {
            user_uid: UserUid::new("attacker-owner"),
        };
        let attacker_sync_id = SyncId::ServerId(ServerId::from(31339));
        let attacker_profile = AIExecutionProfile {
            name: "Attacker Default".to_string(),
            is_default_profile: true,
            execute_commands: ActionPermission::AlwaysAllow,
            ..Default::default()
        };
        let attacker_object = AIExecutionProfileObject::new(
            attacker_sync_id,
            AIExecutionProfileObjectModel::new(attacker_profile),
            StoredObjectMetadata::mock(),
            StoredObjectPermissions {
                owner: attacker_owner,
                permissions_last_updated_ts: None,
                anyone_with_link: None,
                guests: Vec::new(),
            },
        );

        ObjectStoreModel::handle(&app).update(&mut app, move |object_store_model, ctx| {
            object_store_model.create_object(attacker_sync_id, attacker_object, ctx);
        });

        profile_model.read(&app, |model, ctx| {
            let default_profile = model.default_profile(ctx);
            assert_eq!(
                default_profile.sync_id(),
                None,
                "non-owned default profile delivered via ObjectCreated should not be adopted"
            );
            assert_eq!(
                default_profile.data().execute_commands,
                ActionPermission::AlwaysAsk,
                "non-owned profile should not control command approvals"
            );
        });
    })
}

/// See
/// `ignores_non_owned_default_profile_after_initial_load` above for why this
/// is a from-scratch equivalent of the pin's coverage rather than a verbatim
/// port. Equivalent of the pin's `filters_non_owned_non_default_profile_from_list`.
#[test]
fn filters_non_owned_non_default_profile_from_list() {
    App::test((), |mut app| async move {
        install_singletons(&mut app, AuthStateProvider::new_for_test());
        let profile_model = app.add_singleton_model(|ctx| {
            AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
        });

        let attacker_owner = Owner::User {
            user_uid: UserUid::new("attacker-owner"),
        };
        let attacker_sync_id = SyncId::ServerId(ServerId::from(99999));
        let attacker_profile = AIExecutionProfile {
            name: "Attacker Custom".to_string(),
            is_default_profile: false,
            ..Default::default()
        };
        let attacker_object = AIExecutionProfileObject::new(
            attacker_sync_id,
            AIExecutionProfileObjectModel::new(attacker_profile),
            StoredObjectMetadata::mock(),
            StoredObjectPermissions {
                owner: attacker_owner,
                permissions_last_updated_ts: None,
                anyone_with_link: None,
                guests: Vec::new(),
            },
        );

        ObjectStoreModel::handle(&app).update(&mut app, move |object_store_model, ctx| {
            object_store_model.create_object(attacker_sync_id, attacker_object, ctx);
        });

        profile_model.read(&app, |model, ctx| {
            assert!(
                !model.has_multiple_profiles(),
                "non-owned profile should not appear in profile list"
            );
            let all_ids = model.get_all_profile_ids();
            assert_eq!(
                all_ids.len(),
                1,
                "only the default profile should be in the list"
            );
            assert_eq!(all_ids[0], model.default_profile_id());
            assert_eq!(
                model.default_profile(ctx).data().name,
                "Default",
                "surviving profile should be the user's own default, not the attacker's"
            );
        });
    })
}

/// Consumer coverage for `FeatureFlag::LocalComputerUse` -- the flag the whole
/// computer-use settings surface is gated on (the permission dropdown, the
/// computer-use model picker and the computer-use prompt-override slot in the
/// execution profile editor), and the flag that decides whether an explicitly
/// requested computer-use override is honoured for a non-sandboxed CLI agent.
/// `AgentModeComputerUse` used to be hard-disabled in `warp_features`, which
/// made all of this unreachable no matter what this flag said. These assertions
/// fail if the local half is ever switched off again.
#[test]
fn cli_profile_honours_computer_use_override_when_local_computer_use_is_on() {
    let _flag = FeatureFlag::LocalComputerUse.override_enabled(true);

    assert_eq!(
        AIExecutionProfile::create_default_cli_profile(false, Some(true)).computer_use,
        ComputerUsePermission::AlwaysAllow,
        "an explicit computer-use opt-in on an unsandboxed CLI agent should be honoured"
    );
    assert_eq!(
        AIExecutionProfile::create_default_cli_profile(false, Some(false)).computer_use,
        ComputerUsePermission::Never,
        "an explicit opt-out must still win"
    );
}

#[test]
fn cli_profile_refuses_computer_use_override_when_local_computer_use_is_off() {
    let _flag = FeatureFlag::LocalComputerUse.override_enabled(false);

    assert_eq!(
        AIExecutionProfile::create_default_cli_profile(false, Some(true)).computer_use,
        ComputerUsePermission::Never,
        "without the local flag there is no settings surface to opt in with, so the \
         override must not silently grant computer use"
    );
}

// ── Ask-User-Question speedbump dropdown (#11) ──
// The speedbump dropdown selects by *label* (`Dropdown::set_selected_by_name`),
// so the labels must be distinct -- a duplicate would make the dropdown silently
// show the wrong permission as selected.
#[test]
fn ask_user_question_permission_labels_are_distinct() {
    use crate::ai::execution_profiles::AskUserQuestionPermission;
    use std::collections::HashSet;

    let offered = [
        AskUserQuestionPermission::Never,
        AskUserQuestionPermission::AskExceptInAutoApprove,
        AskUserQuestionPermission::AlwaysAsk,
    ];
    let labels: HashSet<&str> = offered.iter().map(|p| p.label()).collect();
    assert_eq!(
        labels.len(),
        offered.len(),
        "each selectable permission needs its own dropdown label"
    );
    assert!(labels.iter().all(|label| !label.is_empty()));

    // `Unknown` is a deserialization catch-all that is never offered in the
    // dropdown, but it must still render as something sensible.
    assert_eq!(
        AskUserQuestionPermission::Unknown.label(),
        AskUserQuestionPermission::AlwaysAsk.label()
    );
}
