use super::*;
use crate::ai::agent::task::TaskId;
use crate::ai::agent::AIAgentActionResultType;
use crate::ai::agent::ReadSkillRequest;
use crate::ai::agent::ReadSkillResult;
use crate::ai::agent::{AIAgentAction, AIAgentActionId, AIAgentActionType};
use crate::ai::blocklist::action_model::AIConversationId;
use crate::ai::skills::{BundledSkillActivation, SkillManager};
use crate::warp_managed_paths_watcher::WarpManagedPathsWatcher;
use ai::agent::action_result::AnyFileContent;
use ai::skills::{parse_skill, ParsedSkill, SkillProvider, SkillReference, SkillScope};
use repo_metadata::{
    repositories::DetectedRepositories, watcher::DirectoryWatcher, RepoMetadataModel,
};
use std::fs;
use std::io::Write;
use tempfile::TempDir;
use warp_core::execution_mode::{AppExecutionMode, ExecutionMode};
use warp_core::features::FeatureFlag;
use warpui::App;
use watcher::HomeDirectoryWatcher;

fn initialize_app(app: &mut App) {
    app.add_singleton_model(|ctx| AppExecutionMode::new(ExecutionMode::App, false, ctx));
    app.add_singleton_model(DirectoryWatcher::new);
    app.add_singleton_model(|_| DetectedRepositories::default());
    app.add_singleton_model(RepoMetadataModel::new);
    app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
    app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
    app.add_singleton_model(SkillManager::new);
}

/// A synthetic bundled skill for activation tests, matching the pin's
/// `read_skill_tests.rs::bundled_skill` helper (`02b53fcd8`) minus the
/// `LocalOrRemotePath` typing (issue #299 — not yet migrated in this fork).
fn bundled_skill(name: &str) -> ParsedSkill {
    ParsedSkill {
        name: name.to_string(),
        description: format!("{name} bundled skill"),
        path: std::path::PathBuf::from(format!("/bundled/skills/{name}/SKILL.md")),
        content: format!("# {name}"),
        line_range: None,
        provider: SkillProvider::Zap,
        scope: SkillScope::Bundled,
    }
}

fn create_test_skill_file(dir: &TempDir, name: &str, description: &str) -> std::path::PathBuf {
    let skill_content = format!(
        r#"---
name: {}
description: {}
---

# {}

## Instructions
Test instructions for this skill.

## Examples
Example usage of the skill.
"#,
        name, description, name
    );

    let skill_dir = dir.path().join(format!(".claude/skills/{}", name));
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_path = skill_dir.join("SKILL.md");
    let mut file = fs::File::create(&skill_path).unwrap();
    file.write_all(skill_content.as_bytes()).unwrap();
    file.flush().unwrap();

    skill_path
}

#[test]
fn test_read_skill_executor_success() {
    let temp_dir = TempDir::new().unwrap();
    let skill_path = create_test_skill_file(&temp_dir, "test-skill", "A test skill");

    App::test((), |mut app| async move {
        initialize_app(&mut app);

        // Populate SkillManager cache with the test skill
        let parsed_skill = parse_skill(&skill_path).expect("Failed to parse test skill");
        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            manager.add_skill_for_testing(parsed_skill);
        });

        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());

        let action = AIAgentAction {
            id: AIAgentActionId::from("test-action-id".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(skill_path.clone()),
            }),
            task_id: TaskId::new("test-task-id".to_string()),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();

            match result {
                AnyActionExecution::Sync(AIAgentActionResultType::ReadSkill(
                    ReadSkillResult::Success { content },
                )) => {
                    assert_eq!(content.file_name, skill_path.to_string_lossy().to_string());
                }
                _ => panic!("Successfully read skill file; should return ReadSkillResult::Success"),
            }
        });
    });
}

#[test]
fn test_read_skill_executor_file_not_found() {
    let temp_dir = TempDir::new().unwrap();
    // Don't create the SKILL.md file
    let skill_path = temp_dir.path().join("SKILL.md");

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());

        let action = AIAgentAction {
            id: AIAgentActionId::from("test-action-id".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(skill_path),
            }),
            task_id: TaskId::new("test-task-id".to_string()),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();

            match result {
                AnyActionExecution::Sync(AIAgentActionResultType::ReadSkill(
                    ReadSkillResult::Error(error_msg),
                )) => {
                    // Should contain an error about file not found or I/O error
                    assert!(!error_msg.is_empty());
                }
                _ => panic!(
                    "Nonexistent SKILL.md file at given path; should return ReadSkillResult::Error"
                ),
            }
        });
    });
}

/// Issue #99 fallback: on a cache miss, if SkillReference::Path points to a
/// validly-shaped skill file, read it straight from disk and succeed (via the
/// Async branch).
#[test]
fn test_read_skill_executor_fallback_reads_disk_on_cache_miss() {
    let temp_dir = TempDir::new().unwrap();
    let skill_path = create_test_skill_file(&temp_dir, "fallback-skill", "Read from disk");

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        // Note: intentionally not calling add_skill_for_testing, to simulate a cache miss.
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());

        let action = AIAgentAction {
            id: AIAgentActionId::from("fallback-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(skill_path.clone()),
            }),
            task_id: TaskId::new("fallback-task".to_string()),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        let execution = executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            result
        });

        let AnyActionExecution::Async {
            execute_future,
            on_complete,
        } = execution
        else {
            panic!("Cache miss with valid skill path should produce Async execution");
        };

        let async_result = execute_future.await;
        let result = app.update(|ctx| on_complete(async_result, ctx));

        match result {
            AIAgentActionResultType::ReadSkill(ReadSkillResult::Success { content }) => {
                assert_eq!(content.file_name, skill_path.to_string_lossy().to_string());
                let body = match &content.content {
                    AnyFileContent::StringContent(s) => s.clone(),
                    AnyFileContent::BinaryContent(_) => {
                        panic!("SKILL.md should be parsed as text")
                    }
                };
                assert!(body.contains("fallback-skill"));
            }
            other => panic!("Fallback should return Success, got: {other:?}"),
        }
    });
}

/// Issue #99 fallback failure path: on a cache miss, if the path shape is valid but
/// the file doesn't exist on disk (e.g. a race where it was deleted after
/// validation), the Async branch's parse_skill fails, and on_complete should return
/// Error.
#[test]
fn test_read_skill_executor_fallback_returns_error_when_file_missing() {
    let temp_dir = TempDir::new().unwrap();
    // Path shape is valid, but SKILL.md was never created.
    let skill_path = temp_dir
        .path()
        .join(".agents/skills/missing-skill/SKILL.md");

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());

        let action = AIAgentAction {
            id: AIAgentActionId::from("missing-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(skill_path),
            }),
            task_id: TaskId::new("missing-task".to_string()),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        let execution = executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            result
        });

        let AnyActionExecution::Async {
            execute_future,
            on_complete,
        } = execution
        else {
            panic!("Legal-shaped skill path should still produce Async execution before disk check");
        };

        let async_result = execute_future.await;
        let result = app.update(|ctx| on_complete(async_result, ctx));

        match result {
            AIAgentActionResultType::ReadSkill(ReadSkillResult::Error(msg)) => {
                assert!(msg.starts_with("Skill not found"));
            }
            other => panic!("Missing file should resolve to Error, got: {other:?}"),
        }
    });
}

/// When the BYOP `read_skill` tool is called with a name:
/// `from_args` stuffs the name into `SkillReference::SkillPath(name)`, and on the
/// executor side, after a cache miss, it's looked up by name and returned as a Sync
/// Success.
#[test]
fn test_read_skill_executor_resolves_by_name() {
    let temp_dir = TempDir::new().unwrap();
    let skill_path = create_test_skill_file(&temp_dir, "byop-named-skill", "Lookup by name");

    App::test((), |mut app| async move {
        initialize_app(&mut app);

        let parsed_skill = parse_skill(&skill_path).expect("Failed to parse test skill");
        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            manager.add_skill_for_testing(parsed_skill);
        });

        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());

        // Simulates BYOP from_args: passing the name in as if it were a path.
        let action = AIAgentAction {
            id: AIAgentActionId::from("name-lookup-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(std::path::PathBuf::from("byop-named-skill")),
            }),
            task_id: TaskId::new("name-lookup-task".to_string()),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            match result {
                AnyActionExecution::Sync(AIAgentActionResultType::ReadSkill(
                    ReadSkillResult::Success { content },
                )) => {
                    assert_eq!(content.file_name, skill_path.to_string_lossy().to_string());
                }
                _ => panic!("Lookup by name should succeed via Sync Success"),
            }
        });
    });
}

/// An unknown name (not in the SkillManager index), after exhausting every
/// fallback: `name_candidate` matches but `find_skill_by_name` returns None, so it
/// continues to the fs fallback — where the path shape is invalid (a plain name has
/// no `/`), resulting directly in a Sync Error.
#[test]
fn test_read_skill_executor_rejects_unknown_name() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());

        let action = AIAgentAction {
            id: AIAgentActionId::from("unknown-name-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(std::path::PathBuf::from("no-such-skill")),
            }),
            task_id: TaskId::new("unknown-name-task".to_string()),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            match result {
                AnyActionExecution::Sync(AIAgentActionResultType::ReadSkill(
                    ReadSkillResult::Error(msg),
                )) => {
                    assert!(msg.starts_with("Skill not found"), "msg={msg}");
                }
                _ => panic!("Unknown name should resolve to Sync Error"),
            }
        });
    });
}

/// Issue #99 safety gate: on a cache miss, if the path doesn't match a skill file
/// shape, it goes straight to the Sync Error branch without triggering any disk
/// read.
#[test]
fn test_read_skill_executor_rejects_non_skill_path_on_cache_miss() {
    let temp_dir = TempDir::new().unwrap();
    // A random markdown file that doesn't sit in the
    // `.<provider>/skills/<name>/SKILL.md` structure. Even though the file exists,
    // the fallback shouldn't read it — extract_skill_parent_directory rejects it.
    let non_skill_path = temp_dir.path().join("random.md");
    fs::write(&non_skill_path, "not a skill").unwrap();

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());

        let action = AIAgentAction {
            id: AIAgentActionId::from("non-skill-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(non_skill_path),
            }),
            task_id: TaskId::new("non-skill-task".to_string()),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            match result {
                AnyActionExecution::Sync(AIAgentActionResultType::ReadSkill(
                    ReadSkillResult::Error(msg),
                )) => {
                    assert!(msg.starts_with("Skill not found"));
                }
                _ => panic!(
                    "Non-skill path on cache miss should return Sync Error, not Async fallback"
                ),
            }
        });
    });
}

/// Ported from the pin's `test_read_skill_executor_reads_enabled_bundled_skill`
/// (`02b53fcd8`): a bundled skill whose activation condition is met is readable
/// by `BundledSkillId` reference. See issue #370.
#[test]
fn test_read_skill_executor_reads_enabled_bundled_skill() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            manager.add_bundled_skill_for_testing(
                "pr-comments",
                bundled_skill("pr-comments"),
                BundledSkillActivation::Always,
            );
        });
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());

        let action = AIAgentAction {
            id: AIAgentActionId::from("test-action-id".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::BundledSkillId("pr-comments".to_string()),
            }),
            task_id: TaskId::new("test-task-id".to_string()),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();

            match result {
                AnyActionExecution::Sync(AIAgentActionResultType::ReadSkill(
                    ReadSkillResult::Success { content },
                )) => {
                    assert_eq!(content.file_name, "/bundled/skills/pr-comments/SKILL.md");
                }
                _ => panic!("Enabled bundled skill should return ReadSkillResult::Success"),
            }
        });
    });
}

/// Ported from the pin's `test_read_skill_executor_rejects_tui_only_skill_in_gui`
/// (`02b53fcd8`): a `TuiOnly` bundled skill is not readable when the app is not
/// running as the TUI. See issue #370.
#[test]
fn test_read_skill_executor_rejects_tui_only_skill_in_gui() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
        let skill_id = "tui-migrate-setup";
        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            manager.add_bundled_skill_for_testing(
                skill_id,
                bundled_skill(skill_id),
                BundledSkillActivation::TuiOnly,
            );
        });
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());
        let action = AIAgentAction {
            id: AIAgentActionId::from(format!("test-action-id-{skill_id}")),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::BundledSkillId(skill_id.to_string()),
            }),
            task_id: TaskId::new(format!("test-task-id-{skill_id}")),
            requires_result: false,
        };
        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            assert!(matches!(
                result,
                AnyActionExecution::Sync(AIAgentActionResultType::ReadSkill(
                    ReadSkillResult::Error(_)
                ))
            ));
        });
    });
}

/// Ported from the pin's
/// `test_read_skill_executor_rejects_warp_control_bundled_skills_when_disabled`
/// (`02b53fcd8`): a `RequiresFeature`-gated bundled skill is not readable while
/// its feature is disabled.
///
/// Pin deviation: the pin drives this through its `warpctrl` bundled skill,
/// whose directory this fork does not ship (`resources/bundled/skills/warpctrl`
/// is absent — see `activation_for_bundled_skill`, and issue #370). The gating
/// flag itself is real here, so the test registers a synthetic bundled skill
/// behind the same `FeatureFlag::WarpControlCli` the pin uses; only the skill
/// content is stood in for, not the mechanism or the flag.
#[test]
fn test_read_skill_executor_rejects_gated_bundled_skill_when_feature_disabled() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
        let _gate = FeatureFlag::WarpControlCli.override_enabled(false);
        let skill_id = "feature-gated-skill";
        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            manager.add_bundled_skill_for_testing(
                skill_id,
                bundled_skill(skill_id),
                BundledSkillActivation::RequiresFeature(FeatureFlag::WarpControlCli),
            );
        });
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new());
        let action = AIAgentAction {
            id: AIAgentActionId::from(format!("test-action-id-{skill_id}")),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::BundledSkillId(skill_id.to_string()),
            }),
            task_id: TaskId::new(format!("test-task-id-{skill_id}")),
            requires_result: false,
        };

        let input = ExecuteActionInput {
            action: &action,
            conversation_id: AIConversationId::new(),
        };

        executor_handle.update(&mut app, |executor, ctx| {
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            assert!(matches!(
                result,
                AnyActionExecution::Sync(AIAgentActionResultType::ReadSkill(
                    ReadSkillResult::Error(_)
                ))
            ));
        });
    });
}
