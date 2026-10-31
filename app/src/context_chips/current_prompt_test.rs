use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use async_trait::async_trait;
use itertools::Itertools;
use parking_lot::Mutex;
use settings::Setting as _;
use warp_core::command::ExitCode;
use warpui::{App, SingletonEntity};
use warpui_extras::user_preferences;

#[cfg(feature = "local_fs")]
use crate::code_review::diff_state::DiffStats;
#[cfg(feature = "local_fs")]
use crate::code_review::git_status_update::{GitRepoStatusModel, GitStatusMetadata};
#[cfg(feature = "local_fs")]
use crate::context_chips::display_chip::GitBranchTrackingStatus;
#[cfg(windows)]
use crate::system::SystemInfo;
use crate::ai::blocklist::agent_view::agent_input_footer::toolbar_item::AgentToolbarItemKind;
use crate::terminal::cli_agent_sessions::{
    CLIAgentInputState, CLIAgentSession, CLIAgentSessionContext, CLIAgentSessionStatus,
    CLIAgentSessionsModel,
};
use crate::terminal::CLIAgent;
use crate::{
    auth::{AuthManager, AuthStateProvider},
    context_chips::{
        context_chip::{Environment, PromptGenerator},
        prompt::Prompt,
        ChipAvailability, ChipDisabledReason, ContextChipKind,
    },
    features::FeatureFlag,
    menu::MenuItem,
    settings::WarpPromptSeparator,
    terminal::{
        model::{
            block::BlockMetadata,
            session::{CommandExecutor, ExecuteCommandOptions, SessionId, SessionInfo, Sessions},
        },
        session_settings::{
            AgentToolbarChipSelection, CLIAgentToolbarChipSelection, SessionSettings,
            ToolbarChipSelection,
        },
        shell::Shell,
        view::PromptPosition,
        History,
    },
};
#[cfg(feature = "local_fs")]
use repo_metadata::DirectoryWatcher;
use warp_completer::completer::{CommandExitStatus, CommandOutput};

use super::{
    effective_command_timeout, effective_poll_interval, poll_decision, ActiveChipSurfaces,
    ChipState, ChipUpdateStatus, CurrentPrompt, PollDecision, PromptContext,
    BACKGROUND_RECHECK_DELAY, DEFAULT_CHIP_COMMAND_TIMEOUT, REMOTE_POLL_INTERVAL,
    TYPING_QUIET_PERIOD, TYPING_RECHECK_DELAY,
};
use std::time::Duration;
#[cfg(feature = "local_fs")]
use crate::code_review::github_repo_model::GitHubRepoModel;
#[cfg(feature = "local_fs")]
use crate::util::git::PrInfo;

#[cfg(feature = "local_fs")]
fn git_status_metadata(branch: &str) -> GitStatusMetadata {
    GitStatusMetadata {
        current_branch_name: branch.to_string(),
        main_branch_name: "main".to_string(),
        stats_against_head: DiffStats::default(),
        branch_tracking_status: GitBranchTrackingStatus::new(branch.to_string(), None, 0, 0),
    }
}

#[test]
fn test_context_menu_items() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [
                    ContextChipKind::WorkingDirectory,
                    ContextChipKind::VirtualEnvironment,
                ],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        // Set a value for the working directory, but not the virtual environment.
        current_prompt.update(&mut app, |current_prompt, ctx| {
            // Ensure there are state entries for the expected chips.
            current_prompt.update_states_with_new_context(ctx);
            current_prompt.update_chip_value(
                &ContextChipKind::WorkingDirectory,
                Some(crate::context_chips::ChipValue::Text(
                    "/path/to/dir".to_string(),
                )),
            );
        });

        app.read(|ctx| {
            let menu_items = current_prompt
                .as_ref(ctx)
                .copy_menu_items(PromptPosition::Input, ctx)
                .into_iter()
                .filter_map(|item| match item {
                    MenuItem::Item(fields) => Some(fields.label().to_string()),
                    _ => None,
                })
                .collect_vec();

            assert_eq!(menu_items, vec!["Copy Working Directory"]);
        })
    });
}

#[test]
fn test_prompt_to_string() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [
                    ContextChipKind::Username,
                    ContextChipKind::VirtualEnvironment,
                    ContextChipKind::WorkingDirectory,
                    ContextChipKind::ShellGitBranch,
                ],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        // Set a value for the working directory, but not the virtual environment.
        current_prompt.update(&mut app, |current_prompt, ctx| {
            // Ensure there are state entries for the expected chips.
            current_prompt.update_states_with_new_context(ctx);
            current_prompt.update_chip_value(
                &ContextChipKind::Username,
                Some(crate::context_chips::ChipValue::Text("user".to_string())),
            );
            current_prompt.update_chip_value(
                &ContextChipKind::WorkingDirectory,
                Some(crate::context_chips::ChipValue::Text(
                    "/path/to/dir".to_string(),
                )),
            );
            current_prompt.update_chip_value(
                &ContextChipKind::ShellGitBranch,
                Some(crate::context_chips::ChipValue::Text(
                    "my-branch".to_string(),
                )),
            );
        });

        app.read(|ctx| {
            let prompt_string = current_prompt.as_ref(ctx).prompt_as_string(ctx);
            // Components should be in order, and missing components should be skipped.
            assert_eq!(prompt_string, "user /path/to/dir git:(my-branch)");
        })
    });
}

#[test]
fn test_fingerprint_skips_contextual_chip_recompute_when_context_is_unchanged() {
    App::test((), |mut app| async move {
        let session_id = SessionId::from(777);
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::WorkingDirectory],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        current_prompt.update(&mut app, |current_prompt, ctx| {
            current_prompt.latest_context = Some(PromptContext {
                active_block_metadata: BlockMetadata::new(
                    Some(session_id),
                    Some("/tmp/project".to_string()),
                ),
                environment: Environment::default(),
            });
            current_prompt.update_states_with_new_context(ctx);

            let state = current_prompt
                .states
                .get(&ContextChipKind::WorkingDirectory)
                .expect("expected working directory state");
            assert_eq!(state.update_status, ChipUpdateStatus::Ready);
            assert!(state.last_fingerprint.is_some());
        });

        current_prompt.update(&mut app, |current_prompt, ctx| {
            current_prompt.update_states_with_new_context(ctx);

            let state = current_prompt
                .states
                .get(&ContextChipKind::WorkingDirectory)
                .expect("expected working directory state");
            assert_eq!(state.update_status, ChipUpdateStatus::Cached);
            assert!(matches!(
                state.last_computed_value.as_ref().and_then(|v| v.as_text()),
                Some("/tmp/project")
            ));
        });
    });
}

#[test]
fn test_typing_only_reports_a_change_when_a_chip_starts_or_stops_rendering() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::KubernetesContext],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        current_prompt.update(&mut app, |current_prompt, _| {
            let aliases = Default::default();
            current_prompt.states.insert(
                ContextChipKind::KubernetesContext,
                ChipState::new(&ContextChipKind::KubernetesContext),
            );

            // Typing a command the kubernetes chip does not care about changes nothing, so the
            // prompt has no reason to rebuild itself.
            assert!(!current_prompt.refresh_should_render("l", &aliases));
            assert!(!current_prompt.refresh_should_render("ls -la", &aliases));

            // The chip appears once the command is a kubernetes one, and only then.
            assert!(current_prompt.refresh_should_render("kubectl get pods", &aliases));
            assert!(!current_prompt.refresh_should_render("kubectl get pods -A", &aliases));

            // And disappears again when the command changes.
            assert!(current_prompt.refresh_should_render("ls", &aliases));
            assert!(!current_prompt.refresh_should_render("ls -la", &aliases));
        });
    });
}

#[test]
fn test_shell_chip_is_disabled_when_required_executable_is_missing() {
    App::test((), |mut app| async move {
        let session_id = SessionId::from(456);
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::ShellGitBranch],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_| History::new(vec![]));
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| AuthStateProvider::new_for_test());
        app.add_singleton_model(AuthManager::new_for_test);
        app.add_singleton_model(|_| crate::settings::manager::SettingsManager::default());
        crate::settings::InputSettings::register(&mut app);
        app.update(crate::settings::AISettings::register_and_subscribe_to_events);
        app.add_singleton_model(crate::workspaces::user_workspaces::UserWorkspaces::default_mock);
        #[cfg(windows)]
        app.add_singleton_model(SystemInfo::new);

        let executor = Arc::new(RecordingCommandExecutor::default());
        let sessions = app.add_model(|ctx| {
            let mut sessions = Sessions::new_for_test().with_command_executor(executor.clone());
            sessions.initialize_bootstrapped_session(
                SessionInfo::new_for_test().with_id(session_id),
                "test command".to_string(),
                vec![],
                None,
                ctx,
            );
            sessions
        });
        let sessions_for_prompt = sessions.clone();
        let current_prompt =
            app.add_model(move |ctx| CurrentPrompt::new(sessions_for_prompt.clone(), ctx));

        let session = app
            .read(|ctx| sessions.as_ref(ctx).get(session_id))
            .expect("session should exist");
        session.load_external_commands().await;
        executor.clear();

        current_prompt.update(&mut app, |current_prompt, ctx| {
            current_prompt.latest_context = Some(PromptContext {
                active_block_metadata: BlockMetadata::new(
                    Some(session_id),
                    Some("/tmp/project".to_string()),
                ),
                environment: Environment::default(),
            });
            current_prompt.update_states_with_new_context(ctx);

            let state = current_prompt
                .states
                .get(&ContextChipKind::ShellGitBranch)
                .expect("expected git branch state");
            assert_eq!(
                state.availability,
                ChipAvailability::Disabled(ChipDisabledReason::RequiresExecutable {
                    command: "git".to_string(),
                })
            );
            assert_eq!(state.update_status, ChipUpdateStatus::Disabled);
            assert!(state.generator_handle.is_none());
            assert!(state.on_click_generator_handle.is_none());
        });

        assert!(executor.commands.lock().is_empty());
    });
}

// Ported from the pinned oracle
// (`02b53fcd8:app/src/context_chips/current_prompt_tests.rs`). The chip lost its
// `gh pr view` shell generator when the fork adopted `GitHubRepoModel`, so the
// runtime policy is now empty and the value is pushed in by the model.
#[test]
fn test_github_pr_chip_runtime_policy_configuration() {
    let _flag_guard = FeatureFlag::GithubPrPromptChip.override_enabled(true);
    let chip = ContextChipKind::GithubPullRequest
        .to_chip()
        .expect("github pr chip should exist");
    let policy = chip.runtime_policy();

    assert!(matches!(
        chip.generator(),
        PromptGenerator::Contextual { .. }
    ));
    assert!(policy.required_executables().is_empty());
    assert_eq!(policy.shell_command_timeout(), None);
    assert!(!policy.suppress_on_failure());
    assert!(policy.fingerprint_inputs().is_empty());
    assert!(policy.invalidate_on_commands().is_empty());
}

#[test]
fn test_invalidating_command_count_unaffected_for_chips_without_invalidate_on_commands() {
    App::test((), |mut app| async move {
        let session_id = SessionId::from(888);
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::WorkingDirectory],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        current_prompt.update(&mut app, |current_prompt, ctx| {
            current_prompt.latest_context = Some(PromptContext {
                active_block_metadata: BlockMetadata::new(
                    Some(session_id),
                    Some("/tmp/project".to_string()),
                ),
                environment: Environment::default(),
            });
            current_prompt.update_states_with_new_context(ctx);

            // WorkingDirectory has no invalidate_on_commands, so the counter should be 0.
            let state = current_prompt
                .states
                .get(&ContextChipKind::WorkingDirectory)
                .expect("expected working directory state");
            assert_eq!(state.invalidating_command_count, 0);
        });
    });
}

#[test]
fn test_disabling_chips() {
    App::test((), |mut app| async move {
        let session_id = SessionId::from(123);
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::ShellGitBranch],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_| History::new(vec![]));
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| AuthStateProvider::new_for_test());
        app.add_singleton_model(AuthManager::new_for_test);

        // Register required singleton models to fix the singleton model error
        app.add_singleton_model(|_| crate::settings::manager::SettingsManager::default());
        crate::settings::InputSettings::register(&mut app);
        app.update(crate::settings::AISettings::register_and_subscribe_to_events);
        app.add_singleton_model(crate::workspaces::user_workspaces::UserWorkspaces::default_mock);
        #[cfg(windows)]
        app.add_singleton_model(SystemInfo::new);

        let executor = Arc::new(RecordingCommandExecutor::default());

        let sessions = app.add_model(|ctx| {
            let mut sessions = Sessions::new_for_test().with_command_executor(executor.clone());
            sessions.initialize_bootstrapped_session(
                SessionInfo::new_for_test().with_id(session_id),
                "test command".to_string(),
                vec![],
                None,
                ctx,
            );
            sessions
        });
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        // Context chips can only be disabled in Classic mode.
        app.update(|ctx| {
            crate::settings::InputSettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings
                    .input_box_type
                    .set_value(crate::settings::InputBoxType::Classic, ctx);
            });
        });

        executor.clear();

        current_prompt
            .update(&mut app, |current_prompt, ctx| {
                current_prompt.latest_context = Some(PromptContext {
                    active_block_metadata: BlockMetadata::new(Some(session_id), None),
                    environment: Environment::default(),
                });
                // This is needed because we set latest_context directly.
                current_prompt.update_states_with_new_context(ctx);
                assert!(current_prompt.are_any_generators_running());
                current_prompt.await_generators(ctx)
            })
            .await;

        // By default, context chips are enabled, so the git branch command should run. It may run
        // twice due to how periodically-refreshing chips are implemented.
        assert!(!executor.commands.lock().is_empty());

        // If PS1 is enabled, the command should not run.
        app.update(|ctx| {
            SessionSettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings.honor_ps1.set_value(true, ctx);
            });
        });
        // Clear the command history right after changing the PS1 setting, to ensure that the
        // CurrentPrompt model has processed the change.
        executor.clear();

        current_prompt.update(&mut app, |current_prompt, ctx| {
            // Ensure that, if the model were going to run generators, it had a chance to.
            current_prompt.update_states_with_new_context(ctx);
            // There may be some shell generators still pending in the background, which won't be
            // directly cancelled. Instead of asserting that no commands run, assert that the
            // CurrentPrompt model is not still trying to run generators.
            assert!(!current_prompt.are_any_generators_running());
        });

        // If context chips are re-enabled, generator commands should start running again.
        app.update(|ctx| {
            SessionSettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings.honor_ps1.set_value(false, ctx);
            });
        });

        current_prompt
            .update(&mut app, |current_prompt, ctx| {
                assert!(current_prompt.are_any_generators_running());
                current_prompt.await_generators(ctx)
            })
            .await;

        assert!(!executor.commands.lock().is_empty());
    });
}

#[test]
fn test_chips_to_run_only_includes_active_surface_configurations() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::Username],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.update(|ctx| {
            SessionSettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .agent_footer_chip_selection
                    .set_value(
                        AgentToolbarChipSelection::Custom {
                            left: vec![AgentToolbarItemKind::ContextChip(
                                ContextChipKind::WorkingDirectory,
                            )],
                            right: vec![],
                        },
                        ctx,
                    )
                    .unwrap();
                settings
                    .cli_agent_footer_chip_selection
                    .set_value(
                        CLIAgentToolbarChipSelection::Custom {
                            left: vec![AgentToolbarItemKind::ContextChip(
                                ContextChipKind::ShellGitBranch,
                            )],
                            right: vec![],
                        },
                        ctx,
                    )
                    .unwrap();
            });
        });

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        app.read(|ctx| {
            let current_prompt = current_prompt.as_ref(ctx);
            let chips_for = |surfaces| current_prompt.chips_to_run_for_surfaces(surfaces, ctx);

            assert!(chips_for(ActiveChipSurfaces::default()).is_empty());
            assert_eq!(
                chips_for(ActiveChipSurfaces {
                    prompt: true,
                    ..Default::default()
                }),
                vec![ContextChipKind::Username]
            );
            assert_eq!(
                chips_for(ActiveChipSurfaces {
                    agent_footer: true,
                    ..Default::default()
                }),
                vec![ContextChipKind::WorkingDirectory]
            );
            assert_eq!(
                chips_for(ActiveChipSurfaces {
                    cli_agent_footer: true,
                    ..Default::default()
                }),
                vec![ContextChipKind::ShellGitBranch]
            );
            assert_eq!(
                chips_for(ActiveChipSurfaces {
                    prompt: true,
                    agent_footer: true,
                    cli_agent_footer: true,
                }),
                vec![
                    ContextChipKind::Username,
                    ContextChipKind::WorkingDirectory,
                    ContextChipKind::ShellGitBranch,
                ]
            );
        });
    });
}

#[test]
fn test_cli_agent_footer_chips_require_a_visible_supported_footer() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Prompt::mock());
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_| History::new(vec![]));
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| AuthStateProvider::new_for_test());
        app.add_singleton_model(AuthManager::new_for_test);
        app.add_singleton_model(|_| crate::settings::manager::SettingsManager::default());
        crate::settings::InputSettings::register(&mut app);
        app.update(crate::settings::AISettings::register_and_subscribe_to_events);
        app.add_singleton_model(crate::workspaces::user_workspaces::UserWorkspaces::default_mock);
        app.add_singleton_model(|_| CLIAgentSessionsModel::new());
        #[cfg(windows)]
        app.add_singleton_model(SystemInfo::new);

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));
        let terminal_view_id = current_prompt.id();
        current_prompt.update(&mut app, |current_prompt, _| {
            current_prompt.terminal_view_id = Some(terminal_view_id);
        });

        let cli_footer_chips =
            |app: &App| app.read(|ctx| current_prompt.as_ref(ctx).chips_to_run(ctx));
        assert!(cli_footer_chips(&app).is_empty());

        let session_for = |agent| CLIAgentSession {
            agent,
            status: CLIAgentSessionStatus::InProgress,
            session_context: CLIAgentSessionContext::default(),
            input_state: CLIAgentInputState::Closed,
            should_auto_toggle_input: false,
            listener: None,
            plugin_version: None,
            remote_host: None,
            draft_text: None,
            custom_command_prefix: None,
            received_rich_notification: false,
        };

        CLIAgentSessionsModel::handle(&app).update(&mut app, |sessions, ctx| {
            sessions.set_session(terminal_view_id, session_for(CLIAgent::Claude), ctx);
        });
        assert_eq!(
            cli_footer_chips(&app),
            app.read(|ctx| {
                SessionSettings::as_ref(ctx)
                    .cli_agent_footer_chip_selection
                    .all_chips()
            })
        );

        CLIAgentSessionsModel::handle(&app).update(&mut app, |sessions, ctx| {
            sessions.set_session(terminal_view_id, session_for(CLIAgent::PhosphorTui), ctx);
        });
        assert!(cli_footer_chips(&app).is_empty());

        CLIAgentSessionsModel::handle(&app).update(&mut app, |sessions, ctx| {
            sessions.set_session(terminal_view_id, session_for(CLIAgent::Claude), ctx);
        });
        crate::settings::AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .should_render_cli_agent_footer
                .set_value(false, ctx)
                .unwrap();
        });
        assert!(cli_footer_chips(&app).is_empty());
    });
}

#[test]
fn test_ps1_without_active_agent_surface_runs_no_footer_generators() {
    let _flag_guard = FeatureFlag::AgentView.override_enabled(true);
    App::test((), |mut app| async move {
        let session_id = SessionId::from(321);
        app.add_singleton_model(|_| Prompt::mock());
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_| History::new(vec![]));
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.update(|ctx| {
            SessionSettings::handle(ctx).update(ctx, |settings, ctx| {
                settings.honor_ps1.set_value(true, ctx).unwrap();
            });
        });
        app.add_singleton_model(|_| AuthStateProvider::new_for_test());
        app.add_singleton_model(AuthManager::new_for_test);
        app.add_singleton_model(|_| crate::settings::manager::SettingsManager::default());
        crate::settings::InputSettings::register(&mut app);
        app.update(crate::settings::AISettings::register_and_subscribe_to_events);
        app.add_singleton_model(crate::workspaces::user_workspaces::UserWorkspaces::default_mock);
        #[cfg(windows)]
        app.add_singleton_model(SystemInfo::new);

        let executor = Arc::new(RecordingCommandExecutor::default());
        let sessions = app.add_model(|ctx| {
            let mut sessions = Sessions::new_for_test().with_command_executor(executor.clone());
            sessions.initialize_bootstrapped_session(
                SessionInfo::new_for_test().with_id(session_id),
                "test command".to_string(),
                vec![],
                None,
                ctx,
            );
            sessions
        });
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        current_prompt
            .update(&mut app, |current_prompt, ctx| {
                current_prompt.latest_context = Some(PromptContext {
                    active_block_metadata: BlockMetadata::new(Some(session_id), None),
                    environment: Environment::default(),
                });
                current_prompt.update_states_with_new_context(ctx);
                current_prompt.await_generators(ctx)
            })
            .await;

        assert!(executor.commands.lock().is_empty());
        app.read(|ctx| {
            assert!(current_prompt.as_ref(ctx).states.is_empty());
        });
    });
}

#[cfg(feature = "local_fs")]
#[test]
fn test_externally_driven_chip_skips_periodic_timer() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::ShellGitBranch],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher_handle = app.add_singleton_model(DirectoryWatcher::new_for_testing);
        let repo_handle = watcher_handle.update(&mut app, |watcher, ctx| {
            watcher
                .add_directory(
                    warp_util::standardized_path::StandardizedPath::from_local_canonicalized(
                        temp_dir.path(),
                    )
                    .unwrap(),
                    ctx,
                )
                .unwrap()
        });
        let git_status = app
            .add_model(move |ctx| GitRepoStatusModel::new_local_for_test(repo_handle, None, ctx));

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        current_prompt.update(&mut app, |cp, ctx| {
            cp.set_git_repo_status(Some(git_status.downgrade()), ctx);
            cp.update_states_with_new_context(ctx);
        });

        app.read(|ctx| {
            let cp = current_prompt.as_ref(ctx);
            let state = cp
                .states
                .get(&ContextChipKind::ShellGitBranch)
                .expect("ShellGitBranch state should exist after set_git_repo_status");
            assert!(
                state.refresh_handle.is_none(),
                "Externally-driven chip should not have a periodic refresh handle"
            );
        });
    });
}

#[cfg(feature = "local_fs")]
#[test]
fn test_git_status_change_updates_chip_value() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::ShellGitBranch],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher_handle = app.add_singleton_model(DirectoryWatcher::new_for_testing);
        let repo_handle = watcher_handle.update(&mut app, |watcher, ctx| {
            watcher
                .add_directory(
                    warp_util::standardized_path::StandardizedPath::from_local_canonicalized(
                        temp_dir.path(),
                    )
                    .unwrap(),
                    ctx,
                )
                .unwrap()
        });

        let initial_metadata = GitStatusMetadata {
            current_branch_name: "main".to_string(),
            main_branch_name: "main".to_string(),
            stats_against_head: DiffStats::default(),
            branch_tracking_status: GitBranchTrackingStatus::new("main".to_string(), None, 0, 0),
        };
        let git_status = app.add_model(move |ctx| {
            GitRepoStatusModel::new_local_for_test(repo_handle, Some(initial_metadata), ctx)
        });

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        // Subscribe to the git status model and run chips.
        current_prompt.update(&mut app, |cp, ctx| {
            cp.set_git_repo_status(Some(git_status.downgrade()), ctx);
            cp.update_states_with_new_context(ctx);
        });

        // Simulate a branch change by updating the model's metadata.
        git_status.update(&mut app, |model, ctx| {
            model.set_metadata_for_test(
                Some(GitStatusMetadata {
                    current_branch_name: "feature-branch".to_string(),
                    main_branch_name: "main".to_string(),
                    stats_against_head: DiffStats::default(),
                    branch_tracking_status: GitBranchTrackingStatus::new(
                        "feature-branch".to_string(),
                        None,
                        0,
                        0,
                    ),
                }),
                ctx,
            );
        });

        app.read(|ctx| {
            let value = current_prompt
                .as_ref(ctx)
                .latest_chip_value(&ContextChipKind::ShellGitBranch);
            assert_eq!(
                value,
                Some(&crate::context_chips::ChipValue::Text(
                    "feature-branch".to_string(),
                )),
                "Chip value should reflect the new branch name after metadata change"
            );
        });
    });
}

#[cfg(feature = "local_fs")]
#[test]
fn test_git_status_change_updates_branch_status_chip_value() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::GitBranchStatus],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher_handle = app.add_singleton_model(DirectoryWatcher::new_for_testing);
        let repo_handle = watcher_handle.update(&mut app, |watcher, ctx| {
            watcher
                .add_directory(
                    warp_util::standardized_path::StandardizedPath::from_local_canonicalized(
                        temp_dir.path(),
                    )
                    .unwrap(),
                    ctx,
                )
                .unwrap()
        });

        let git_status = app
            .add_model(move |ctx| GitRepoStatusModel::new_local_for_test(repo_handle, None, ctx));
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        current_prompt.update(&mut app, |cp, ctx| {
            cp.set_git_repo_status(Some(git_status.downgrade()), ctx);
            cp.update_states_with_new_context(ctx);
        });

        let branch_tracking_status = GitBranchTrackingStatus::new(
            "feature-branch".to_string(),
            Some("origin/feature-branch".to_string()),
            3,
            1,
        );
        git_status.update(&mut app, |model, ctx| {
            model.set_metadata_for_test(
                Some(GitStatusMetadata {
                    current_branch_name: "feature-branch".to_string(),
                    main_branch_name: "main".to_string(),
                    stats_against_head: DiffStats::default(),
                    branch_tracking_status: branch_tracking_status.clone(),
                }),
                ctx,
            );
        });

        app.read(|ctx| {
            let value = current_prompt
                .as_ref(ctx)
                .latest_chip_value(&ContextChipKind::GitBranchStatus);
            assert_eq!(
                value,
                Some(&crate::context_chips::ChipValue::GitBranchStatus(
                    branch_tracking_status,
                )),
                "Branch status chip should reflect ahead and behind metadata"
            );
        });
    });
}

/// #1167: counts the shell commands a chip with an on-click generator (`GitBranchStatus`:
/// status command + branch-list command) spawns across prompt updates.
///
/// Before the fix every fetch spawned both commands (so a new block with unchanged output cost
/// 2 spawns) and a repeated update for the same block restarted everything again.
#[cfg(feature = "local_fs")]
#[test]
fn test_unchanged_chip_refetch_does_not_respawn_on_click_command() {
    App::test((), |mut app| async move {
        let session_id = SessionId::from(1167);
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::GitBranchStatus],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_| History::new(vec![]));
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| AuthStateProvider::new_for_test());
        app.add_singleton_model(AuthManager::new_for_test);
        app.add_singleton_model(|_| crate::settings::manager::SettingsManager::default());
        crate::settings::InputSettings::register(&mut app);
        app.update(crate::settings::AISettings::register_and_subscribe_to_events);
        app.add_singleton_model(crate::workspaces::user_workspaces::UserWorkspaces::default_mock);
        #[cfg(windows)]
        app.add_singleton_model(SystemInfo::new);
        app.update(|ctx| {
            crate::settings::InputSettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings
                    .input_box_type
                    .set_value(crate::settings::InputBoxType::Classic, ctx);
            });
        });

        let executor = Arc::new(RecordingCommandExecutor::default());
        let sessions = app.add_model(|ctx| {
            let mut sessions = Sessions::new_for_test().with_command_executor(executor.clone());
            sessions.initialize_bootstrapped_session(
                SessionInfo::new_for_test().with_id(session_id),
                "test command".to_string(),
                vec![],
                None,
                ctx,
            );
            sessions
        });
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));
        executor.clear();

        let context = || PromptContext {
            active_block_metadata: BlockMetadata::new(Some(session_id), Some("/a".to_string())),
            environment: Environment::default(),
        };
        // Runs one prompt update and waits for the chip's commands, including the on-click
        // command that is spawned from the main command's completion.
        async fn settle(
            app: &mut App,
            current_prompt: &warpui::ModelHandle<CurrentPrompt>,
            block_id: &str,
            context: PromptContext,
        ) {
            current_prompt
                .update(app, |cp, ctx| {
                    cp.apply_context(block_id.to_string(), context, ctx);
                    cp.await_generators(ctx)
                })
                .await;
            warpui::r#async::Timer::after(std::time::Duration::from_millis(50)).await;
            current_prompt
                .update(app, |cp, ctx| cp.await_generators(ctx))
                .await;
        }

        // First prompt: the status command and the branch-list command both run.
        settle(&mut app, &current_prompt, "block-1", context()).await;
        assert_eq!(executor.commands.lock().len(), 2, "status + branch list");

        // New block, same inputs, same output: the status is re-read (diff/ahead-behind can
        // move) but the unchanged answer must not trigger another branch-list spawn.
        settle(&mut app, &current_prompt, "block-2", context()).await;
        assert_eq!(
            executor.commands.lock().len(),
            3,
            "an unchanged refetch must not re-run the on-click command"
        );

        // The same block reporting the same context again changes no input: nothing restarts.
        settle(&mut app, &current_prompt, "block-2", context()).await;
        assert_eq!(
            executor.commands.lock().len(),
            3,
            "a repeated update for the same block must not spawn anything"
        );
    });
}

/// #1166: the branch chip must follow the branch the shell reported in precmd immediately,
/// instead of waiting for the git-status refresh, and the refresh must then take over.
#[cfg(feature = "local_fs")]
#[test]
fn test_branch_status_chip_is_seeded_from_precmd_branch_before_git_status_refresh() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::GitBranchStatus],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher_handle = app.add_singleton_model(DirectoryWatcher::new_for_testing);
        let repo_handle = watcher_handle.update(&mut app, |watcher, ctx| {
            watcher
                .add_directory(
                    warp_util::standardized_path::StandardizedPath::from_local_canonicalized(
                        temp_dir.path(),
                    )
                    .unwrap(),
                    ctx,
                )
                .unwrap()
        });
        let git_status = app
            .add_model(move |ctx| GitRepoStatusModel::new_local_for_test(repo_handle, None, ctx));
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        // The git-status model has published nothing yet; the shell's precmd says "feat/seed".
        current_prompt.update(&mut app, |cp, ctx| {
            cp.set_git_repo_status(Some(git_status.downgrade()), ctx);
            cp.latest_context = Some(PromptContext {
                active_block_metadata: BlockMetadata::new(None, Some("/tmp/project".to_string())),
                environment: Environment::default().with_git_branch("feat/seed"),
            });
            cp.update_states_with_new_context(ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                current_prompt
                    .as_ref(ctx)
                    .latest_chip_value(&ContextChipKind::GitBranchStatus),
                Some(&crate::context_chips::ChipValue::GitBranchStatus(
                    GitBranchTrackingStatus::without_counts("feat/seed".to_string(), None),
                )),
                "branch chip should show the precmd branch before the git-status refresh"
            );
        });

        // The refresh lands and replaces the seed with the full status.
        let full_status = GitBranchTrackingStatus::new(
            "feat/seed".to_string(),
            Some("origin/feat/seed".to_string()),
            2,
            0,
        );
        git_status.update(&mut app, |model, ctx| {
            model.set_metadata_for_test(
                Some(GitStatusMetadata {
                    current_branch_name: "feat/seed".to_string(),
                    main_branch_name: "main".to_string(),
                    stats_against_head: DiffStats::default(),
                    branch_tracking_status: full_status.clone(),
                }),
                ctx,
            );
        });
        app.read(|ctx| {
            assert_eq!(
                current_prompt
                    .as_ref(ctx)
                    .latest_chip_value(&ContextChipKind::GitBranchStatus),
                Some(&crate::context_chips::ChipValue::GitBranchStatus(
                    full_status
                )),
            );
        });

        // A later prompt for the same branch must not clobber the counts with a bare seed.
        current_prompt.update(&mut app, |cp, ctx| {
            cp.update_states_with_new_context(ctx);
            let Some(crate::context_chips::ChipValue::GitBranchStatus(status)) =
                cp.latest_chip_value(&ContextChipKind::GitBranchStatus)
            else {
                panic!("expected a branch status value");
            };
            assert_eq!(status.ahead, 2);
        });
    });
}

/// A [`CommandExecutor`] implementation that records which commands were run, but does not
/// execute them.
#[derive(Debug, Default)]
struct RecordingCommandExecutor {
    commands: Mutex<Vec<String>>,
    response_queue: Mutex<VecDeque<CommandOutput>>,
}

impl RecordingCommandExecutor {
    pub fn with_success_responses(responses: impl IntoIterator<Item = &'static str>) -> Self {
        Self::with_outputs(
            responses
                .into_iter()
                .map(Self::success_output)
                .collect::<Vec<_>>(),
        )
    }

    pub fn with_outputs(outputs: impl IntoIterator<Item = CommandOutput>) -> Self {
        Self {
            commands: Mutex::default(),
            response_queue: Mutex::new(outputs.into_iter().collect()),
        }
    }

    pub fn success_output(stdout: impl AsRef<[u8]>) -> CommandOutput {
        CommandOutput {
            stdout: stdout.as_ref().to_vec(),
            stderr: vec![],
            status: CommandExitStatus::Success,
            exit_code: Some(ExitCode::from(0)),
        }
    }

    pub fn failure_output(stderr: impl AsRef<[u8]>, exit_code: ExitCode) -> CommandOutput {
        CommandOutput {
            stdout: vec![],
            stderr: stderr.as_ref().to_vec(),
            status: CommandExitStatus::Failure,
            exit_code: Some(exit_code),
        }
    }

    pub fn clear(&self) {
        self.commands.lock().clear();
    }
}

#[async_trait]
impl CommandExecutor for RecordingCommandExecutor {
    async fn execute_command(
        &self,
        command: &str,
        _shell: &Shell,
        _current_directory_path: Option<&str>,
        _environment_variables: Option<HashMap<String, String>>,
        _execute_command_options: ExecuteCommandOptions,
    ) -> anyhow::Result<CommandOutput> {
        self.commands.lock().push(command.to_string());
        let output = self
            .response_queue
            .lock()
            .pop_front()
            .unwrap_or_else(|| Self::success_output("test"));
        Ok(output)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn supports_parallel_command_execution(&self) -> bool {
        false
    }
}

/// Ported from the pinned oracle
/// (`02b53fcd8:app/src/context_chips/current_prompt_tests.rs`).
#[cfg(feature = "local_fs")]
#[test]
fn test_git_status_pr_info_updates_github_pr_chip_value() {
    let _flag_guard = FeatureFlag::GithubPrPromptChip.override_enabled(true);
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| {
            Prompt::mock_with(
                [ContextChipKind::GithubPullRequest],
                false,
                WarpPromptSeparator::None,
            )
        });
        app.add_singleton_model(SessionSettings::new_with_defaults);
        app.add_singleton_model(|_ctx| {
            settings::PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        app.add_singleton_model(|_| {
            settings::PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });

        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher_handle = app.add_singleton_model(DirectoryWatcher::new_for_testing);
        let repo_handle = watcher_handle.update(&mut app, |watcher, ctx| {
            watcher
                .add_directory(
                    warp_util::standardized_path::StandardizedPath::from_local_canonicalized(
                        temp_dir.path(),
                    )
                    .unwrap(),
                    ctx,
                )
                .unwrap()
        });

        let git_status = app.add_model(move |ctx| {
            GitRepoStatusModel::new_local_for_test(
                repo_handle,
                Some(git_status_metadata("feature-a")),
                ctx,
            )
        });
        let github_repo_model = {
            let git_status = git_status.clone();
            app.add_model(move |ctx| GitHubRepoModel::new_local_for_test(git_status, ctx))
        };

        let sessions = app.add_model(|_| Sessions::new_for_test());
        let current_prompt = app.add_model(move |ctx| CurrentPrompt::new(sessions, ctx));

        current_prompt.update(&mut app, |cp, ctx| {
            cp.set_git_repo_status(Some(git_status.downgrade()), ctx);
            cp.set_github_repo_model(Some(github_repo_model.downgrade()), ctx);
            cp.update_states_with_new_context(ctx);
        });

        github_repo_model.update(&mut app, |model, ctx| {
            model.set_pr_info_for_test(
                Some(PrInfo {
                    number: 123,
                    url: "https://github.com/warp/warp/pull/123".to_string(),
                    state: "OPEN".to_string(),
                    draft: true,
                    base_branch: "main".to_string(),
                }),
                ctx,
            );
        });

        app.read(|ctx| {
            let value = current_prompt
                .as_ref(ctx)
                .latest_chip_value(&ContextChipKind::GithubPullRequest);
            assert_eq!(
                value,
                Some(&crate::context_chips::ChipValue::Text(
                    "https://github.com/warp/warp/pull/123".to_string(),
                )),
            );
        });

        // Clearing the model's PR info propagates to the chip.
        github_repo_model.update(&mut app, |model, ctx| {
            model.set_pr_info_for_test(None, ctx);
        });

        app.read(|ctx| {
            let value = current_prompt
                .as_ref(ctx)
                .latest_chip_value(&ContextChipKind::GithubPullRequest);
            assert_eq!(
                value, None,
                "PR chip should clear when the model's cached PR info is cleared"
            );
        });

        // Detaching the model clears the chip state entirely.
        github_repo_model.update(&mut app, |model, ctx| {
            model.set_pr_info_for_test(
                Some(PrInfo {
                    number: 456,
                    url: "https://github.com/warp/warp/pull/456".to_string(),
                    state: "OPEN".to_string(),
                    draft: false,
                    base_branch: "main".to_string(),
                }),
                ctx,
            );
        });

        current_prompt.update(&mut app, |cp, ctx| {
            cp.set_github_repo_model(None, ctx);
        });

        app.read(|ctx| {
            let value = current_prompt
                .as_ref(ctx)
                .latest_chip_value(&ContextChipKind::GithubPullRequest);
            assert_eq!(
                value, None,
                "detaching the GitHub repo model should clear the PR chip"
            );
        });
    });
}

#[test]
fn poll_runs_when_foreground_and_idle() {
    assert_eq!(poll_decision(true, None), PollDecision::Run);
    assert_eq!(
        poll_decision(true, Some(Duration::from_secs(10))),
        PollDecision::Run
    );
}

#[test]
fn poll_defers_for_background_terminals() {
    assert_eq!(
        poll_decision(false, None),
        PollDecision::Defer(BACKGROUND_RECHECK_DELAY)
    );
    // Hidden wins over idle: a hidden tab never starts a chip command.
    assert_eq!(
        poll_decision(false, Some(Duration::from_secs(60))),
        PollDecision::Defer(BACKGROUND_RECHECK_DELAY)
    );
}

#[test]
fn poll_defers_while_typing() {
    assert_eq!(
        poll_decision(true, Some(Duration::from_millis(200))),
        PollDecision::Defer(TYPING_RECHECK_DELAY)
    );
    assert_eq!(
        poll_decision(true, Some(TYPING_QUIET_PERIOD)),
        PollDecision::Run
    );
}

#[test]
fn remote_sessions_poll_slowly_but_fast_chips_are_untouched() {
    let thirty = Duration::from_secs(30);
    assert_eq!(effective_poll_interval(thirty, false), thirty);
    assert_eq!(effective_poll_interval(thirty, true), REMOTE_POLL_INTERVAL);
    let clock = Duration::from_secs(1);
    assert_eq!(effective_poll_interval(clock, true), clock);
    // Never shorten an already longer interval (the date chip).
    let date = Duration::from_secs(30 * 60);
    assert_eq!(effective_poll_interval(date, true), date);
}

#[test]
fn chip_commands_always_run_under_a_timeout() {
    // A chip whose policy sets no timeout still gets the default, so a hung helper cannot
    // run (and pile up on every refresh) forever.
    assert_eq!(
        effective_command_timeout(None),
        DEFAULT_CHIP_COMMAND_TIMEOUT
    );
    // A chip's own timeout is honoured, whether shorter or longer than the default.
    let short = Duration::from_secs(2);
    assert_eq!(effective_command_timeout(Some(short)), short);
    let long = DEFAULT_CHIP_COMMAND_TIMEOUT * 2;
    assert_eq!(effective_command_timeout(Some(long)), long);
}
