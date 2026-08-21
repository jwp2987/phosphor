use std::path::PathBuf;

use uuid::Uuid;

use settings::Setting as _;
use warp_util::path::EscapeChar;
use warpui::{App, EntityId, ModelHandle, SingletonEntity};

use warp_core::execution_mode::ExecutionMode;

use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::{
    GlobalResourceHandles, GlobalResourceHandlesProvider, LaunchMode,
    ai::{
        agent::conversation::AIConversationId,
        blocklist::{
            CommandExecutionPermissionAllowedReason,
            permissions::{
                CommandExecutionPermission, CommandExecutionPermissionDeniedReason,
                FileReadPermission, FileReadPermissionAllowedReason,
                FileReadPermissionDeniedReason, FileWritePermission,
                FileWritePermissionAllowedReason, FileWritePermissionDeniedReason,
            },
        },
        execution_profiles::{
            ActionPermission, WriteToPtyPermission, profiles::AIExecutionProfilesModel,
        },
        mcp::templatable_manager::TemplatableMCPServerManager,
    },
    auth::AuthStateProvider,
    cloud_object::model::persistence::ObjectStoreModel,
    cloud_object::update_manager::UpdateManager,
    network::NetworkStatus,
    settings::{AISettings, AgentModeCommandExecutionPredicate, PrivacySettings},
    test_util::settings::initialize_settings_for_tests_with_mode,
    workspaces::{user_workspaces::UserWorkspaces, workspace::SandboxedAgentSettings},
};

use super::{BlocklistAIHistoryModel, BlocklistAIPermissions};

struct PermissionsTestState {
    convo_id: AIConversationId,
    permissions: ModelHandle<BlocklistAIPermissions>,
    history: ModelHandle<BlocklistAIHistoryModel>,
    terminal_view_id: EntityId,
    user_workspaces: ModelHandle<UserWorkspaces>,
    profile_model: ModelHandle<AIExecutionProfilesModel>,
}

fn initialize_permissions_test(app: &mut App) -> PermissionsTestState {
    initialize_permissions_test_with_mode(app, ExecutionMode::App, false)
}

fn initialize_permissions_test_sandboxed(app: &mut App) -> PermissionsTestState {
    let state = initialize_permissions_test_with_mode(app, ExecutionMode::Sdk, true);
    state.profile_model.update(app, |model, ctx| {
        let profile_id = *model.default_profile(ctx).id();
        model.apply_cli_profile_defaults_for_test(profile_id, true, ctx);
    });
    state
}

fn initialize_permissions_test_with_mode(
    app: &mut App,
    mode: ExecutionMode,
    is_sandboxed: bool,
) -> PermissionsTestState {
    initialize_settings_for_tests_with_mode(app, mode, is_sandboxed);
    let global_resource_handles = GlobalResourceHandles::mock(app);
    app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));
    let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], vec![], &[]));
    app.add_singleton_model(|_| CLIAgentSessionsModel::new());
    let permissions = app.add_singleton_model(BlocklistAIPermissions::new);
    let terminal_view_id = EntityId::new();
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    app.add_singleton_model(|_| NetworkStatus::new());
    app.add_singleton_model(UpdateManager::mock);
    app.add_singleton_model(ObjectStoreModel::mock);
    app.add_singleton_model(|_| TemplatableMCPServerManager::default());
    let profile_model = app.add_singleton_model(|ctx| {
        AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
    });
    app.add_singleton_model(PrivacySettings::mock);
    let user_workspaces = app.add_singleton_model(UserWorkspaces::default_mock);

    let conversation_id = history.update(app, |history_model, ctx| {
        history_model.start_new_conversation(terminal_view_id, false, false, ctx)
    });

    PermissionsTestState {
        convo_id: conversation_id,
        permissions,
        history,
        terminal_view_id,
        user_workspaces,
        profile_model,
    }
}

#[test]
fn test_can_read_files_empty_paths() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        permissions.read(&app, |model, ctx| {
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![],
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                FileReadPermission::Allowed(FileReadPermissionAllowedReason::ExplicitlyAllowlisted)
            ));
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_can_read_files_workspace_settings_override_profile() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            model.set_read_files(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAllow,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/test/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                FileReadPermission::Allowed(
                    FileReadPermissionAllowedReason::AutoreadSettingEnabled
                )
            ));
        });

        // Now set the workspace to AlwaysAsk
        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.read_files_setting = Some(ActionPermission::AlwaysAsk);
                },
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/test/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(
                matches!(
                    result,
                    FileReadPermission::Denied(FileReadPermissionDeniedReason::AlwaysAskEnabled)
                ),
                "the workspace setting should override the profile setting"
            );
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_can_read_files_profile_workspace_allowlist_interaction() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Set up profile with allowlist and AlwaysAsk
        profile_model.update(&mut app, |model, ctx| {
            model.set_read_files(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAsk,
                ctx,
            );
            model.add_to_directory_allowlist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &PathBuf::from("/profile/allowed"),
                ctx,
            );
        });

        // Test that files in profile's allowlist are allowed
        permissions.read(&app, |model, ctx| {
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/profile/allowed/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                FileReadPermission::Allowed(FileReadPermissionAllowedReason::ExplicitlyAllowlisted)
            ));

            // Test that files not in profile's allowlist are denied
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/not/allowed/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                FileReadPermission::Denied(FileReadPermissionDeniedReason::AlwaysAskEnabled)
            ));
        });

        // Set up workspace with AlwaysAsk but no allowlist
        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.read_files_setting = Some(ActionPermission::AlwaysAsk);
                    settings.read_files_allowlist = None;
                },
                ctx,
            );
        });

        // Test that the user's profile is respected when there's no workspace allowlist
        permissions.read(&app, |model, ctx| {
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/profile/allowed/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                FileReadPermission::Allowed(FileReadPermissionAllowedReason::ExplicitlyAllowlisted)
            ));
        });

        // Set up workspace with AlwaysAsk and a different allowlist
        user_workspaces.update(&mut app, |model, ctx| {
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.read_files_setting = Some(ActionPermission::AlwaysAsk);
                    settings.read_files_allowlist = Some(vec![PathBuf::from("/workspace/allowed")]);
                },
                ctx,
            );
        });

        // Test that workspace allowlist takes precedence
        permissions.read(&app, |model, ctx| {
            // Files in workspace allowlist should be allowed
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/workspace/allowed/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                FileReadPermission::Allowed(FileReadPermissionAllowedReason::ExplicitlyAllowlisted)
            ));

            // Files in profile allowlist but not workspace allowlist should be denied
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/profile/allowed/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                FileReadPermission::Denied(FileReadPermissionDeniedReason::AlwaysAskEnabled)
            ));

            // Files in neither allowlist should be denied
            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/not/allowed/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                FileReadPermission::Denied(FileReadPermissionDeniedReason::AlwaysAskEnabled)
            ));
        });
    })
}

#[test]
fn test_can_write_files() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            terminal_view_id,
            convo_id,
            permissions,
            profile_model,
            ..
        } = initialize_permissions_test(&mut app);

        // Test AgentDecides setting
        profile_model.update(&mut app, |model, ctx| {
            model.set_apply_code_diffs(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AgentDecides,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_write_files(&convo_id, &[], Some(terminal_view_id), ctx);
            assert!(!result.is_allowed());
            assert!(
                matches!(
                    result,
                    FileWritePermission::Denied(FileWritePermissionDeniedReason::AgentDecided)
                ),
                "not allowed because AgentDecides right now just means ask"
            );
        });

        // Test AlwaysAllow setting
        profile_model.update(&mut app, |model, ctx| {
            model.set_apply_code_diffs(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAllow,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_write_files(&convo_id, &[], Some(terminal_view_id), ctx);
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                FileWritePermission::Allowed(
                    FileWritePermissionAllowedReason::AutowriteSettingEnabled
                )
            ));
        });

        // Test AlwaysAsk setting
        profile_model.update(&mut app, |model, ctx| {
            model.set_apply_code_diffs(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAsk,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_write_files(&convo_id, &[], Some(terminal_view_id), ctx);
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                FileWritePermission::Denied(FileWritePermissionDeniedReason::AlwaysAskEnabled)
            ));
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_can_write_files_workspace_settings_override_profile() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Set profile to AlwaysAllow
        profile_model.update(&mut app, |model, ctx| {
            model.set_apply_code_diffs(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAllow,
                ctx,
            );
        });

        // Test that profile setting is respected when no workspace setting
        permissions.read(&app, |model, ctx| {
            let result = model.can_write_files(&convo_id, &[], Some(terminal_view_id), ctx);
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                FileWritePermission::Allowed(
                    FileWritePermissionAllowedReason::AutowriteSettingEnabled
                )
            ));
        });

        // Set workspace to AlwaysAsk
        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.apply_code_diffs_setting = Some(ActionPermission::AlwaysAsk);
                },
                ctx,
            );
        });

        // Test that workspace setting overrides profile
        permissions.read(&app, |model, ctx| {
            let result = model.can_write_files(&convo_id, &[], Some(terminal_view_id), ctx);
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                FileWritePermission::Denied(FileWritePermissionDeniedReason::AlwaysAskEnabled)
            ));
        });
    })
}

#[test]
fn test_can_write_files_mcp_config_always_denied() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            terminal_view_id,
            convo_id,
            permissions,
            profile_model,
            ..
        } = initialize_permissions_test(&mut app);

        // Even with AlwaysAllow, writing to an MCP config must be denied.
        profile_model.update(&mut app, |model, ctx| {
            model.set_apply_code_diffs(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAllow,
                ctx,
            );
        });

        let mcp_config_paths = vec![
            PathBuf::from("/project/.mcp.json"),
            PathBuf::from("/project/.warp/.mcp.json"),
            PathBuf::from("/project/.codex/config.toml"),
            // The guard is handed the RAW strings the model emitted
            // (`request_file_edits.rs`, `should_autoexecute`), while the write resolves them
            // first via `host_native_absolute_path` — tilde expansion, cwd join, lexical
            // normalisation. Every spelling below resolves to a protected file at write time
            // and used to slip past the guard, because Claude is the one provider whose home
            // config name (`.claude.json`) differs from its project config name
            // (`.mcp.json`), leaving it with only an absolute-path equality match.
            PathBuf::from("~/.claude.json"),
            PathBuf::from(".claude.json"),
            PathBuf::from("/home/someone/projects/../.claude.json"),
            PathBuf::from("./.claude.json"),
            // These already resolved by luck, via the project-config suffix match. Kept so a
            // future change to the suffix logic cannot quietly drop them.
            PathBuf::from("~/.mcp.json"),
            PathBuf::from("~/.warp/.mcp.json"),
            PathBuf::from("~/.codex/config.toml"),
            PathBuf::from("~/.agents/.mcp.json"),
        ];

        for path in mcp_config_paths {
            permissions.read(&app, |model, ctx| {
                let result = model.can_write_files(
                    &convo_id,
                    std::slice::from_ref(&path),
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    !result.is_allowed(),
                    "expected MCP config path {path:?} to be denied"
                );
                assert!(
                    matches!(
                        result,
                        FileWritePermission::Denied(FileWritePermissionDeniedReason::ProtectedPath)
                    ),
                    "expected ProtectedPath denial for {path:?}, got {result:?}"
                );
            });
        }

        // Negative controls: normalising paths must not turn the guard into "deny
        // everything". With `apply_code_diffs` on AlwaysAllow these have to come back
        // allowed, which also proves the denials above came from the protected-path guard
        // and not from the autonomy setting.
        for path in [
            PathBuf::from("/project/src/main.rs"),
            PathBuf::from("~/notes.md"),
            PathBuf::from("README.md"),
            PathBuf::from("/project/../other/mcp.json"),
        ] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_write_files(
                    &convo_id,
                    std::slice::from_ref(&path),
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    result.is_allowed(),
                    "expected unprotected path {path:?} to stay writable, got {result:?}"
                );
            });
        }
    })
}

/// The four command allow/denylist mutators on `BlocklistAIPermissions` must write the store
/// that `can_autoexecute_command` actually reads.
///
/// They used to write `AISettings.agent_mode_command_execution_*`, which no permission
/// decision consults — inherited from this fork's base (`0dbd3d567`), which predates
/// upstream's move of these lists into execution profiles. The pin already writes the default
/// profile (`42effe840:app/src/ai/blocklist/permissions.rs:997-1050`), so this is a port
/// catch-up rather than a divergence.
///
/// The defect was invisible because all four call sites hang off settings-page editors that
/// are constructed but never rendered, so this test is the only thing standing between the
/// mutators and a silent regression: it goes through the public API and asserts on the
/// permission *decision*, not on where the value was stored.
#[test]
fn test_command_autoexecution_mutators_reach_enforcement() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
            model.set_execute_commands(profile_id, &ActionPermission::AlwaysAllow, ctx);
        });

        let rm_rule = AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap();

        // Baseline: nothing is denied yet.
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                result.is_allowed(),
                "expected `rm file.txt` to be allowed before the denylist entry, got {result:?}"
            );
        });

        permissions.update(&mut app, |model, ctx| {
            model
                .add_command_to_autoexecution_denylist(rm_rule.clone(), ctx)
                .unwrap();
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                matches!(
                    result,
                    CommandExecutionPermission::Denied(
                        CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                    )
                ),
                "denylist mutator did not reach the store enforcement reads, got {result:?}"
            );
        });

        permissions.update(&mut app, |model, ctx| {
            model.remove_command_from_denylist(&rm_rule, ctx).unwrap();
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                result.is_allowed(),
                "removal from the denylist did not reach enforcement, got {result:?}"
            );
        });

        // The allowlist half is only consulted when the setting is AlwaysAsk.
        profile_model.update(&mut app, |model, ctx| {
            let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
            model.set_execute_commands(profile_id, &ActionPermission::AlwaysAsk, ctx);
        });

        let git_rule = AgentModeCommandExecutionPredicate::new_regex("git .*").unwrap();

        permissions.update(&mut app, |model, ctx| {
            model
                .add_command_to_autoexecution_allowlist(git_rule.clone(), ctx)
                .unwrap();
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                matches!(
                    result,
                    CommandExecutionPermission::Allowed(
                        CommandExecutionPermissionAllowedReason::ExplicitlyAllowlisted
                    )
                ),
                "allowlist mutator did not reach the store enforcement reads, got {result:?}"
            );
        });

        permissions.update(&mut app, |model, ctx| {
            model
                .remove_command_from_autoexecution_allowlist(&git_rule, ctx)
                .unwrap();
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                !result.is_allowed(),
                "removal from the allowlist did not reach enforcement, got {result:?}"
            );
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_can_autoexecute_command_workspace_settings_override_profile() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Set profile to AlwaysAllow
        profile_model.update(&mut app, |model, ctx| {
            model.set_execute_commands(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAllow,
                ctx,
            );
        });

        // Test that profile setting is respected when no workspace setting
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Allowed(
                    CommandExecutionPermissionAllowedReason::AlwaysAllowed
                )
            ));
        });

        // Set workspace to AlwaysAsk
        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.execute_commands_setting = Some(ActionPermission::AlwaysAsk);
                },
                ctx,
            );
        });

        // Test that workspace setting overrides profile
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Denied(
                    CommandExecutionPermissionDeniedReason::AlwaysAskEnabled
                )
            ));
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_can_autoexecute_command_denylist_precedence() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Set up profile with denylist
        profile_model.update(&mut app, |model, ctx| {
            model.add_to_command_denylist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });

        // Test that profile denylist is respected when no workspace denylist
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Denied(
                    CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                )
            ));
        });

        // Set workspace denylist
        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.execute_commands_denylist = Some(vec![
                        AgentModeCommandExecutionPredicate::new_regex("git .*").unwrap(),
                    ]);
                },
                ctx,
            );
        });

        // Test that workspace denylist overrides profile denylist
        permissions.read(&app, |model, ctx| {
            // git commands should now be denied
            let result = model.can_autoexecute_command(
                &convo_id,
                "git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Denied(
                    CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                )
            ));

            // The org denylist ADDS to the profile's own; it does not replace it, so `rm`
            // must still be denied. This assertion was inverted when the file was ported
            // (it asserted `!matches!`, i.e. replace semantics) and is restored here to the
            // pin's (`42effe840:app/src/ai/blocklist/permissions_tests.rs:651-668`).
            // `get_execute_commands_denylist_for_profile` (`permissions.rs`) merges, so the
            // inverted form encoded the defect the merge was written to prevent. It could
            // not fail today — the `git status` assertion above fails first, because
            // `current_team()` is stubbed to `None` and the workspace denylist is never
            // read, which is why this test is `#[ignore]`d — but a test that documents the
            // wrong expected behaviour is worse than no test: the day `current_team()`
            // gains a producer, the honest failure is the org list not being consulted, not
            // the merge working.
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                matches!(
                    result,
                    CommandExecutionPermission::Denied(
                        CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                    )
                ),
                "user denylist entries should be merged with org denylist, not replaced"
            );
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_can_autoexecute_command_allowlist_precedence() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Set up profile with AlwaysAsk and allowlist
        profile_model.update(&mut app, |model, ctx| {
            model.set_execute_commands(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAsk,
                ctx,
            );
            model.add_to_command_allowlist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &AgentModeCommandExecutionPredicate::new_regex("git .*").unwrap(),
                ctx,
            );
        });

        // Test that profile allowlist is respected when no workspace allowlist
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Allowed(
                    CommandExecutionPermissionAllowedReason::ExplicitlyAllowlisted
                )
            ));
        });

        // Set workspace with AlwaysAsk and different allowlist
        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.execute_commands_setting = Some(ActionPermission::AlwaysAsk);
                    settings.execute_commands_allowlist = Some(vec![
                        AgentModeCommandExecutionPredicate::new_regex("ls .*").unwrap(),
                    ]);
                },
                ctx,
            );
        });

        // Test that workspace allowlist overrides profile allowlist
        permissions.read(&app, |model, ctx| {
            // git commands should now be denied (not in workspace allowlist)
            let result = model.can_autoexecute_command(
                &convo_id,
                "git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Denied(
                    CommandExecutionPermissionDeniedReason::AlwaysAskEnabled
                )
            ));

            // ls commands should now be allowed
            let result = model.can_autoexecute_command(
                &convo_id,
                "ls -l",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Allowed(
                    CommandExecutionPermissionAllowedReason::ExplicitlyAllowlisted
                )
            ));
        });
    })
}

/// Auto-approve bypasses the *user* denylist when
/// `auto_approve_bypasses_command_denylist` is set.
///
/// The pin's test also asserted the other half -- that the **workspace/org**
/// denylist survives the bypass -- and that half is deliberately not ported.
/// `UserWorkspaces::current_team()` returns `None` unconditionally in this
/// fork (cloud teams / org policy declined, `DECLINED.md` #445), so
/// `ai_autonomy_settings()` always yields defaults and the org denylist is
/// inert by construction. Asserting on it here would assert nothing --
/// exactly the fake coverage `script/check_stub_coverage` exists to prevent.
/// Restore it together with a real local workspace-policy source (#445).
#[test]
fn test_can_autoexecute_command_auto_approve_bypasses_user_denylist() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            history,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Add a denylist rule that matches the test command.
        profile_model.update(&mut app, |model, ctx| {
            model.add_to_command_denylist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });

        // Enable auto-approve for this conversation.
        history.update(&mut app, |history, ctx| {
            history.toggle_autoexecute_override(&convo_id, terminal_view_id, ctx);
        });

        permissions.read(&app, |model, ctx| {
            let user_denylisted = model.can_autoexecute_command(
                &convo_id,
                "rm important.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(matches!(
                user_denylisted,
                CommandExecutionPermission::Allowed(
                    CommandExecutionPermissionAllowedReason::RunToCompletion
                )
            ));
        });
    })
}

#[test]
fn test_can_autoexecute_command_auto_approve_respects_local_denylist_when_bypass_disabled() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            history,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            model.add_to_command_denylist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .auto_approve_bypasses_command_denylist
                    .set_value(false, ctx)
                    .expect("setting should update");
            });
        });
        history.update(&mut app, |history, ctx| {
            history.toggle_autoexecute_override(&convo_id, terminal_view_id, ctx);
        });

        permissions.read(&app, |model, ctx| {
            let denied = model.can_autoexecute_command(
                &convo_id,
                "rm important.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(matches!(
                denied,
                CommandExecutionPermission::Denied(
                    CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                )
            ));

            let allowed = model.can_autoexecute_command(
                &convo_id,
                "echo hello",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(matches!(
                allowed,
                CommandExecutionPermission::Allowed(
                    CommandExecutionPermissionAllowedReason::RunToCompletion
                )
            ));
        });
    })
}

#[test]
fn test_can_autoexecute_command_auto_approve_allows_non_denylisted() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            history,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Enable run-to-completion override for the conversation.
        history.update(&mut app, |history, ctx| {
            history.toggle_autoexecute_override(&convo_id, terminal_view_id, ctx);
        });

        // Since the command is not denylisted, the override should allow execution with RunToCompletion.
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "echo hello",
                EscapeChar::Backslash,
                true,        // read-only command
                Some(false), // not risky
                Some(terminal_view_id),
                ctx,
            );
            assert!(result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Allowed(
                    CommandExecutionPermissionAllowedReason::RunToCompletion
                )
            ));
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_can_write_to_pty() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Set profile to AlwaysAllow
        profile_model.update(&mut app, |model, ctx| {
            model.set_write_to_pty(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &WriteToPtyPermission::AlwaysAllow,
                ctx,
            );
        });

        // Test that profile setting is respected when no workspace setting
        permissions.read(&app, |model, ctx| {
            let result = model.can_write_to_pty(&convo_id, Some(terminal_view_id), ctx);
            assert_eq!(result, WriteToPtyPermission::AlwaysAllow);
        });

        // Set workspace to AlwaysAsk
        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.write_to_pty_setting = Some(WriteToPtyPermission::AlwaysAsk);
                },
                ctx,
            );
        });

        // Test that workspace setting overrides profile
        permissions.read(&app, |model, ctx| {
            let result = model.can_write_to_pty(&convo_id, Some(terminal_view_id), ctx);
            assert_eq!(result, WriteToPtyPermission::AlwaysAsk);
        });
    })
}

#[test]
fn test_can_use_mcp_server_always_allow_no_denylist() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        let server_uuid = Uuid::new_v4();

        profile_model.update(&mut app, |model, ctx| {
            model.set_mcp_permissions(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAllow,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            // Any server should be allowed when AlwaysAllow and not denylisted.
            assert!(model.can_use_mcp_server(
                &convo_id,
                Some(server_uuid),
                Some(terminal_view_id),
                ctx
            ));
            // None UUID should also be allowed (no denylist match possible).
            assert!(model.can_use_mcp_server(&convo_id, None, Some(terminal_view_id), ctx));
        });
    })
}

#[test]
fn test_can_use_mcp_server_always_allow_with_denylist() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        let server_uuid = Uuid::new_v4();
        let other_uuid = Uuid::new_v4();

        profile_model.update(&mut app, |model, ctx| {
            model.set_mcp_permissions(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAllow,
                ctx,
            );
            model.add_to_mcp_denylist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &server_uuid,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            // Denylisted server should be denied.
            assert!(!model.can_use_mcp_server(
                &convo_id,
                Some(server_uuid),
                Some(terminal_view_id),
                ctx
            ));
            // Non-denylisted server should be allowed.
            assert!(model.can_use_mcp_server(
                &convo_id,
                Some(other_uuid),
                Some(terminal_view_id),
                ctx
            ));
        });
    })
}

#[test]
fn test_can_use_mcp_server_always_ask_with_allowlist() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        let server_uuid = Uuid::new_v4();
        let other_uuid = Uuid::new_v4();

        profile_model.update(&mut app, |model, ctx| {
            model.set_mcp_permissions(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAsk,
                ctx,
            );
            model.add_to_mcp_allowlist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &server_uuid,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            // Allowlisted server should be allowed.
            assert!(model.can_use_mcp_server(
                &convo_id,
                Some(server_uuid),
                Some(terminal_view_id),
                ctx
            ));
            // Non-allowlisted server should be denied.
            assert!(!model.can_use_mcp_server(
                &convo_id,
                Some(other_uuid),
                Some(terminal_view_id),
                ctx
            ));
            // None UUID should be denied.
            assert!(!model.can_use_mcp_server(&convo_id, None, Some(terminal_view_id), ctx));
        });
    })
}

#[test]
fn test_can_use_mcp_server_always_ask_denylist_overrides_allowlist() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        let server_uuid = Uuid::new_v4();

        profile_model.update(&mut app, |model, ctx| {
            model.set_mcp_permissions(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AlwaysAsk,
                ctx,
            );
            model.add_to_mcp_allowlist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &server_uuid,
                ctx,
            );
            model.add_to_mcp_denylist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &server_uuid,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            // Both allowlisted and denylisted: denylist wins.
            assert!(!model.can_use_mcp_server(
                &convo_id,
                Some(server_uuid),
                Some(terminal_view_id),
                ctx
            ));
        });
    })
}

#[test]
fn test_can_use_mcp_server_agent_decides() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        let server_uuid = Uuid::new_v4();
        let other_uuid = Uuid::new_v4();

        profile_model.update(&mut app, |model, ctx| {
            model.set_mcp_permissions(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AgentDecides,
                ctx,
            );
            model.add_to_mcp_allowlist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &server_uuid,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            // Allowlisted and not denylisted should be allowed.
            assert!(model.can_use_mcp_server(
                &convo_id,
                Some(server_uuid),
                Some(terminal_view_id),
                ctx
            ));
            // Not allowlisted should be denied.
            assert!(!model.can_use_mcp_server(
                &convo_id,
                Some(other_uuid),
                Some(terminal_view_id),
                ctx
            ));
        });
    })
}

#[test]
fn test_can_use_mcp_server_agent_decides_denylist_overrides_allowlist() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        let server_uuid = Uuid::new_v4();

        profile_model.update(&mut app, |model, ctx| {
            model.set_mcp_permissions(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &ActionPermission::AgentDecides,
                ctx,
            );
            model.add_to_mcp_allowlist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &server_uuid,
                ctx,
            );
            model.add_to_mcp_denylist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &server_uuid,
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            // Both allowlisted and denylisted: denylist wins.
            assert!(!model.can_use_mcp_server(
                &convo_id,
                Some(server_uuid),
                Some(terminal_view_id),
                ctx
            ));
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_sandboxed_mode_allows_read_write_files() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            terminal_view_id,
            ..
        } = initialize_permissions_test_sandboxed(&mut app);

        // Set workspace to AlwaysAsk
        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.apply_code_diffs_setting = Some(ActionPermission::AlwaysAsk);
                    settings.read_files_setting = Some(ActionPermission::AlwaysAsk);
                },
                ctx,
            );
        });

        // In sandboxed mode the workspace read/write restrictions are bypassed,
        // so the profile's AlwaysAllow setting takes effect.
        permissions.read(&app, |model, ctx| {
            let result = model.can_write_files(&convo_id, &[], Some(terminal_view_id), ctx);
            assert!(
                result.is_allowed(),
                "write files should be allowed in sandboxed mode (workspace restriction bypassed)"
            );
            assert!(matches!(
                result,
                FileWritePermission::Allowed(
                    FileWritePermissionAllowedReason::AutowriteSettingEnabled
                )
            ));

            let result = model.can_read_files_with_conversation(
                &convo_id,
                vec![PathBuf::from("/test/file.txt")],
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                result.is_allowed(),
                "read files should be allowed in sandboxed mode (workspace restriction bypassed)"
            );
            assert!(matches!(
                result,
                FileReadPermission::Allowed(
                    FileReadPermissionAllowedReason::AutoreadSettingEnabled
                )
            ));
        });
    })
}

#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_sandboxed_denylist_used_in_sandboxed_mode() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            history,
            permissions,
            user_workspaces,
            terminal_view_id,
            ..
        } = initialize_permissions_test_sandboxed(&mut app);

        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            // Regular workspace denylist blocks "git .*".
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.execute_commands_denylist = Some(vec![
                        AgentModeCommandExecutionPredicate::new_regex("git .*").unwrap(),
                    ]);
                },
                ctx,
            );
            // Sandboxed denylist blocks "rm .*" instead.
            model.update_sandboxed_agent_settings(
                |settings| {
                    *settings = Some(SandboxedAgentSettings {
                        execute_commands_denylist: Some(vec![
                            AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                        ]),
                    });
                },
                ctx,
            );
        });

        history.update(&mut app, |history, ctx| {
            history.toggle_autoexecute_override(&convo_id, terminal_view_id, ctx);
        });
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .auto_approve_bypasses_command_denylist
                    .set_value(false, ctx)
                    .expect("setting should update");
            });
        });
        permissions.read(&app, |model, ctx| {
            // "git status" should be allowed: the regular denylist is not consulted in
            // sandboxed mode, so only the sandboxed denylist ("rm .*") applies.
            let result = model.can_autoexecute_command(
                &convo_id,
                "git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(matches!(
                result,
                CommandExecutionPermission::Allowed(
                    CommandExecutionPermissionAllowedReason::RunToCompletion
                )
            ));

            // "rm file.txt" should be denied by the sandboxed denylist.
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(
                matches!(
                    result,
                    CommandExecutionPermission::Denied(
                        CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                    )
                ),
                "rm file.txt should be denied by the sandboxed denylist"
            );
        });
    })
}

#[test]
fn test_denylist_matches_multiline_commands() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        // Add denylist rule for rm
        profile_model.update(&mut app, |model, ctx| {
            model.add_to_command_denylist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });

        // Single-line rm command should be denied
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(!result.is_allowed());
            assert!(matches!(
                result,
                CommandExecutionPermission::Denied(
                    CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                )
            ));
        });

        // Multiline rm command with backslash continuations should also be denied (POSIX)
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file1.txt \\\nfile2.txt \\\nfile3.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                !result.is_allowed(),
                "multiline rm command should be denied by denylist"
            );
            assert!(matches!(
                result,
                CommandExecutionPermission::Denied(
                    CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                )
            ));
        });

        // Multiline rm command with backtick continuations should also be denied (PowerShell)
        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file1.txt `\nfile2.txt `\nfile3.txt",
                EscapeChar::Backtick,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                !result.is_allowed(),
                "multiline rm command with backtick continuations should be denied by denylist"
            );
            assert!(matches!(
                result,
                CommandExecutionPermission::Denied(
                    CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                )
            ));
        });
    })
}

#[test]
fn test_can_autoexecute_command_denylist_matches_env_prefixed_commands() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
            model.set_execute_commands(profile_id, &ActionPermission::AlwaysAllow, ctx);
            model.add_to_command_denylist(
                profile_id,
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });

        for command in [
            "X=1 rm file.txt",
            "echo ok && X=1 rm file.txt",
            "echo $(X=1 rm file.txt)",
            // A value may contain `=`. The pin's strip required the assignment to hold
            // exactly one `=` (`split('=').count() == 2`), so every spelling below reached
            // the denylist with the prefix still attached and matched no `rm` rule.
            // Confirmed against bash, dash and zsh with an `rm` shim: all four spellings
            // below run `rm`.
            "FOO=a=b rm file.txt",
            "A1_b=x=y=z rm file.txt",
            "X=1 FOO=a=b rm file.txt",
            "FOO=$(echo a=b) rm file.txt",
        ] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    EscapeChar::Backslash,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Denied(
                            CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                        )
                    ),
                    "{command:?} should be denied by the rm denylist, got {result:?}"
                );
            });
        }

        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "X=1 git status",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(matches!(
                result,
                CommandExecutionPermission::Allowed(
                    CommandExecutionPermissionAllowedReason::AlwaysAllowed
                )
            ));
        });

        // Negative controls: none of these is an assignment, so the strip must NOT fire.
        // Every one of bash, dash and zsh reports "command not found" for the prefix itself
        // and never runs `rm` — verified with an `rm` shim — so leaving them unstripped
        // withholds no protection. The pin's `split('=').count() == 2` test stripped all
        // three and produced a `rm file.txt` candidate for a command line that cannot run
        // `rm`; that over-match is what this narrowing removes.
        for command in [
            "1FOO=b rm file.txt",
            "FOO-1=x rm file.txt",
            "FOO.1=x rm file.txt",
            "=b rm file.txt",
        ] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    EscapeChar::Backslash,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Allowed(
                            CommandExecutionPermissionAllowedReason::AlwaysAllowed
                        )
                    ),
                    "{command:?} does not run `rm` in any shell, so the env-var strip must \
                     not treat its first word as an assignment; got {result:?}"
                );
            });
        }
    })
}

#[test]
fn test_can_autoexecute_command_denylist_matches_quoted_command_names() {
    // Regression test for the one-quote-character denylist bypass. Every command below is
    // executed by the shell as `rm -rf ~`; before `denylist_match_candidates` existed the
    // denylist matched the text exactly as typed, so an `rm .*` rule caught only the first
    // spelling and a model could evade any user or org rule by adding one quote.
    //
    // NOTE: this is a deliberate divergence from the pin (`42effe840`), which has the same
    // hole. If a re-pin makes this test fail, the pinned code has been restored verbatim and
    // the bypass is back -- reinstate the fix, do not delete the test.
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
            model.set_execute_commands(profile_id, &ActionPermission::AlwaysAllow, ctx);
            model.add_to_command_denylist(
                profile_id,
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });

        for command in [
            // the spelling that already worked, as a control
            "rm -rf ~",
            // fully quoted command name
            "\"rm\" -rf ~",
            "'rm' -rf ~",
            // adjacent concatenation of quoted and unquoted segments
            "r\"m\" -rf ~",
            "\"r\"m -rf ~",
            "'r'\"m\" -rf ~",
            // backslash escapes: mid-word, and the leading form the parser keeps for aliases
            "r\\m -rf ~",
            "\\rm -rf ~",
            // leading `$` before a quoted segment (ANSI-C / locale-translation quoting)
            "$'rm' -rf ~",
            "$\"rm\" -rf ~",
            // quoting in the arguments rather than the command name
            "rm \"-rf\" ~",
            // quoting combined with the env-var prefix, and inside compound commands
            "X=1 \"rm\" -rf ~",
            "echo ok && 'rm' -rf ~",
            "echo $(\"rm\" -rf ~)",
            "echo `\"rm\" -rf ~`",
            // env-var prefix with a quoted *name*, and with a quoted value
            "\"X\"=1 rm file.txt",
            "X=\"1\" rm file.txt",
        ] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    EscapeChar::Backslash,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Denied(
                            CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                        )
                    ),
                    "{command:?} runs `rm` and should be denied by the `rm .*` rule, got {result:?}"
                );
            });
        }

        // Unquoting must not make the denylist match things it should not: a quoted `git` is
        // still `git`, and no amount of normalisation turns it into `rm`.
        for command in ["\"git\" status", "'git' status", "g\"i\"t status"] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    EscapeChar::Backslash,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Allowed(
                            CommandExecutionPermissionAllowedReason::AlwaysAllowed
                        )
                    ),
                    "{command:?} should not be caught by the `rm .*` rule, got {result:?}"
                );
            });
        }
    })
}

#[test]
fn test_can_autoexecute_command_denylist_normalisation_never_falls_open() {
    // `denylist_match_candidates` only ever *adds* spellings; the text as typed stays in the
    // candidate set. Both halves below pin that, because the natural way to write the fix --
    // replacing the command text with its normalised form -- silently stops matching rules
    // written against the original text. The previous helper did exactly that with the
    // env-var prefix, which is what the first half covers.
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
            model.set_execute_commands(profile_id, &ActionPermission::AlwaysAllow, ctx);
            // A rule written against the env-var prefix itself.
            model.add_to_command_denylist(
                profile_id,
                &AgentModeCommandExecutionPredicate::new_regex("X=1 rm .*").unwrap(),
                ctx,
            );
            // A rule written against a command whose *name* cannot be resolved without
            // running the shell, so normalisation has nothing to offer and the as-typed
            // text is the only thing that can match.
            model.add_to_command_denylist(
                profile_id,
                &AgentModeCommandExecutionPredicate::new_regex(r"\$\(echo rm\) -rf /").unwrap(),
                ctx,
            );
        });

        for command in ["X=1 rm file.txt", "$(echo rm) -rf /"] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    EscapeChar::Backslash,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Denied(
                            CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                        )
                    ),
                    "{command:?} matches a denylist rule as typed and must stay denied \
                     regardless of what normalisation can or cannot resolve, got {result:?}"
                );
            });
        }
    })
}

#[test]
fn test_can_autoexecute_command_denylist_matches_line_continuations() {
    // Regression test for a one-newline denylist bypass that survived the quoting fix, because
    // it was created three lines *above* the fix rather than by it.
    //
    // `can_autoexecute_command` normalises line continuations before parsing. It used to
    // replace `\<newline>` with a *space*, which is not what any shell does -- a continuation
    // is removed, so `r\<newline>m -rf ~` runs `rm`, exactly as `r\m` does. Substituting a
    // space produced `r m -rf ~`, whose first word is `r`, so the whole candidate set was
    // built from a command that does not exist and no `rm` rule could match. Confirmed against
    // bash with an `rm()` shim: every command below calls `rm`.
    //
    // Note this defeated the fix's own `r\m` case: the rewrite runs first, so the parser (which
    // drops `\`+newline correctly, `simple/parser.rs`) never saw the continuation at all.
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
            model.set_execute_commands(profile_id, &ActionPermission::AlwaysAllow, ctx);
            model.add_to_command_denylist(
                profile_id,
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });

        for (command, escape_char) in [
            // continuation splitting the command *name* -- the bypass
            ("r\\\nm -rf ~", EscapeChar::Backslash),
            // the same, inside double quotes
            ("\"r\\\nm\" -rf ~", EscapeChar::Backslash),
            // continuation after the name, which worked before only by accident (the inserted
            // space happened to land where a separator already was)
            ("rm\\\n -rf ~", EscapeChar::Backslash),
            // PowerShell spells the continuation with a backtick
            ("r`\nm -rf ~", EscapeChar::Backtick),
            ("\"r`\nm\" -rf ~", EscapeChar::Backtick),
        ] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    escape_char,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Denied(
                            CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                        )
                    ),
                    "{command:?} runs `rm` once the line continuation is removed and should be \
                     denied by the `rm .*` rule, got {result:?}"
                );
            });
        }

        // Removing the continuation must not fabricate a command name that was not there:
        // `g\<newline>it status` is `git status`, and joining across a continuation that had a
        // real word boundary either side stays two words.
        for (command, escape_char) in [
            ("g\\\nit status", EscapeChar::Backslash),
            ("git \\\nstatus", EscapeChar::Backslash),
            ("g`\nit status", EscapeChar::Backtick),
        ] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    escape_char,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Allowed(
                            CommandExecutionPermissionAllowedReason::AlwaysAllowed
                        )
                    ),
                    "{command:?} does not run `rm` and should not be denied, got {result:?}"
                );
            });
        }
    })
}

#[test]
fn test_can_autoexecute_command_denylist_matches_embedded_newlines() {
    // Regression test for a universal one-newline bypass of *every* rule ending in `.*`.
    //
    // Denylist rules are compiled as `^{rule}$` (`settings/ai.rs`) and matched with the `regex`
    // crate, where `.` does not match `\n` and `$` anchors the end of the *haystack*, not the
    // end of a line. So appending one harmless argument containing a newline -- which the
    // shell keeps inside a single command because it is quoted or escaped -- made `rm .*` stop
    // matching a command that still runs `rm -rf ~`. That was true of every rule and every
    // command, not just `rm`. Confirmed against bash with an `rm()` shim.
    //
    // `denylist_match_candidates` now adds a line-break-flattened spelling of each candidate.
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
            model.set_execute_commands(profile_id, &ActionPermission::AlwaysAllow, ctx);
            model.add_to_command_denylist(
                profile_id,
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });

        for command in [
            // one extra argument that is nothing but a newline and a letter
            "rm -rf ~ \"\nx\"",
            "rm -rf ~ '\nx'",
            // newline inside an argument rather than at its start
            "rm \"-rf\nx\" ~",
            // and combined with the quoting bypass this function already covers
            "\"rm\" -rf ~ \"\nx\"",
        ] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    EscapeChar::Backslash,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Denied(
                            CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted
                        )
                    ),
                    "{command:?} runs `rm` and must not escape the `rm .*` rule by carrying a \
                     newline, got {result:?}"
                );
            });
        }

        // Flattening line breaks must not make unrelated commands match: a newline inside an
        // argument of a command that is not `rm` stays not-`rm`, and a rule anchored at `^`
        // cannot be satisfied by text that merely mentions `rm` after the flattening.
        for command in [
            "echo \"a\nrm -rf ~\"",
            "git commit -m \"line one\nline two\"",
        ] {
            permissions.read(&app, |model, ctx| {
                let result = model.can_autoexecute_command(
                    &convo_id,
                    command,
                    EscapeChar::Backslash,
                    false,
                    None,
                    Some(terminal_view_id),
                    ctx,
                );
                assert!(
                    matches!(
                        result,
                        CommandExecutionPermission::Allowed(
                            CommandExecutionPermissionAllowedReason::AlwaysAllowed
                        )
                    ),
                    "{command:?} does not run `rm` and should not be denied, got {result:?}"
                );
            });
        }
    })
}

// Vacuous here: `update_ai_autonomy_settings` writes `execute_commands_denylist = Some(vec![])`
// into the *team's* organization settings, but `UserWorkspaces::current_team()` is hard-`None`
// in this fork, so `workspace_autonomy_settings()` returns `Default` and
// `get_execute_commands_denylist_for_profile` always takes the `None => user_denylist` arm.
// The `Some(org_denylist)` merge arm this was written to guard (kept verbatim from the pin,
// `42effe840:permissions.rs:410-421`) is unreachable, so the assertion below holds against the
// pre-fix code too. Ignored rather than deleted, matching the nine siblings above: the merge
// logic is correct and the test becomes a real regression test the day #445 gives
// `current_team()` a producer. See DECLINED.md, "Workspace / team AI-autonomy and
// sandboxed-agent overrides".
#[test]
#[ignore = "workspace/team AI-autonomy overrides are dropped in the BYOP fork: UserWorkspaces::current_team() is stubbed to None (no cloud teams), so permissions fall back to profile settings by design"]
fn test_empty_org_denylist_allows_user_entries() {
    App::test((), |mut app| async move {
        let PermissionsTestState {
            convo_id,
            permissions,
            user_workspaces,
            profile_model,
            terminal_view_id,
            ..
        } = initialize_permissions_test(&mut app);

        profile_model.update(&mut app, |model, ctx| {
            model.add_to_command_denylist(
                *model.active_profile(Some(terminal_view_id), ctx).id(),
                &AgentModeCommandExecutionPredicate::new_regex("rm .*").unwrap(),
                ctx,
            );
        });

        user_workspaces.update(&mut app, |model, ctx| {
            model.setup_test_workspace(ctx);
            model.update_ai_autonomy_settings(
                |settings| {
                    settings.execute_commands_denylist = Some(vec![]);
                },
                ctx,
            );
        });

        permissions.read(&app, |model, ctx| {
            let result = model.can_autoexecute_command(
                &convo_id,
                "rm file.txt",
                EscapeChar::Backslash,
                false,
                None,
                Some(terminal_view_id),
                ctx,
            );
            assert!(
                !result.is_allowed(),
                "user denylist entry should be active even when org denylist is empty"
            );
        });
    })
}
