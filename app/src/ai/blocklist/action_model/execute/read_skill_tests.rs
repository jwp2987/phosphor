use super::*;
use crate::ai::agent::AIAgentActionResultType;
use crate::ai::agent::ReadSkillRequest;
use crate::ai::agent::ReadSkillResult;
use crate::ai::agent::task::TaskId;
use crate::ai::agent::{AIAgentAction, AIAgentActionId, AIAgentActionType};
use crate::ai::blocklist::action_model::AIConversationId;
use crate::ai::skills::{BundledSkillActivation, SkillManager};
use crate::terminal::model::session::{BootstrapSessionType, SessionInfo, Sessions};
use crate::terminal::model_events::ModelEventDispatcher;
use crate::warp_managed_paths_watcher::WarpManagedPathsWatcher;
use ai::agent::action_result::AnyFileContent;
use ai::skills::{ParsedSkill, SkillProvider, SkillReference, SkillScope, parse_skill};
use repo_metadata::{
    RepoMetadataModel, repositories::DetectedRepositories, watcher::DirectoryWatcher,
};
use std::fs;
use std::io::Write;
use tempfile::TempDir;
use warp_core::SessionId;
use warp_core::execution_mode::{AppExecutionMode, ExecutionMode};
use warp_core::features::FeatureFlag;
use warp_util::local_or_remote_path::LocalOrRemotePath;
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

/// Builds a minimal local `ActiveSession` for tests that don't care about
/// session/host wiring — the vast majority of `ReadSkillExecutor` coverage.
fn build_local_active_session(app: &mut App) -> ModelHandle<ActiveSession> {
    let sessions = app.add_model(|_| Sessions::new_for_test());
    let (_events_tx, events_rx) = async_channel::unbounded();
    let model_events =
        app.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx));
    app.add_model(|ctx| ActiveSession::new(sessions, model_events, ctx))
}

/// Builds an `ActiveSession` whose active session is a `WarpifiedRemote`
/// session connected to `host_id` (the remote-server handshake has already
/// resolved a host id, mirroring what `RemoteServerManager` does once
/// connected). Used by the defect-fix regression test below: before the fix,
/// `ReadSkillExecutor` ignored this entirely and always resolved bundled
/// skills against the local catalog.
fn build_remote_active_session(
    app: &mut App,
    host_id: warp_core::HostId,
) -> ModelHandle<ActiveSession> {
    let session_id = SessionId::from(1);
    let sessions = app.add_model(|_| Sessions::new_for_test());
    sessions.update(app, |sessions, _| {
        sessions.register_session_for_test(
            SessionInfo::new_for_test()
                .with_id(session_id)
                .with_session_type(BootstrapSessionType::WarpifiedRemote),
        );
        sessions
            .get(session_id)
            .expect("just registered")
            .set_remote_host_id(Some(host_id));
    });

    let (_events_tx, events_rx) = async_channel::unbounded();
    let model_events =
        app.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx));
    model_events.update(app, |dispatcher, _| {
        dispatcher.set_active_session_id(session_id);
    });

    app.add_model(|ctx| ActiveSession::new(sessions, model_events, ctx))
}

/// A synthetic bundled skill for activation tests, matching the pin's
/// `read_skill_tests.rs::bundled_skill` helper (`02b53fcd8`).
fn bundled_skill(name: &str) -> ParsedSkill {
    bundled_skill_with_content(name, &format!("# {name}"))
}

/// Like [`bundled_skill`], but with caller-controlled content — used to tell
/// apart which catalog (local vs. a specific remote host) a lookup actually
/// resolved against.
fn bundled_skill_with_content(name: &str, content: &str) -> ParsedSkill {
    ParsedSkill {
        name: name.to_string(),
        description: format!("{name} bundled skill"),
        path: LocalOrRemotePath::Local(std::path::PathBuf::from(format!(
            "/bundled/skills/{name}/SKILL.md"
        ))),
        content: content.to_string(),
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

        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("test-action-id".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(skill_path.clone())),
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
        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("test-action-id".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(skill_path)),
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
    // The fallback is confined to what a warm cache could have surfaced, so the session
    // has to actually be working in the directory that owns the skill. This is the
    // realistic shape of issue #99 anyway: a skill sitting in the repo you are in, which
    // the watcher has not indexed yet.
    let working_directory = temp_dir.path().to_string_lossy().to_string();

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        // Note: intentionally not calling add_skill_for_testing, to simulate a cache miss.
        let active_session = build_local_active_session(&mut app);
        active_session.update(&mut app, |session, _ctx| {
            session.set_current_working_directory_for_test(working_directory);
        });
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("fallback-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(skill_path.clone())),
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
    let working_directory = temp_dir.path().to_string_lossy().to_string();

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let active_session = build_local_active_session(&mut app);
        active_session.update(&mut app, |session, _ctx| {
            session.set_current_working_directory_for_test(working_directory);
        });
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("missing-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(skill_path)),
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
            panic!(
                "Legal-shaped skill path should still produce Async execution before disk check"
            );
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

        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        // Simulates BYOP from_args: passing the name in as if it were a path.
        let action = AIAgentAction {
            id: AIAgentActionId::from("name-lookup-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(std::path::PathBuf::from(
                    "byop-named-skill",
                ))),
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
        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("unknown-name-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(std::path::PathBuf::from(
                    "no-such-skill",
                ))),
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
        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("non-skill-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(non_skill_path)),
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

/// Scope gate on the issue-#99 fallback: a path with a perfectly valid skill *shape*
/// that belongs to no directory this session works in is refused outright, without a
/// disk read. Shape is not permission — `extract_skill_parent_directory` accepts any
/// prefix, so before this gate a model could name
/// `/home/someone-else/.agents/skills/x/SKILL.md` and have it read back, with
/// `should_autoexecute` unconditionally true and no permission prompt anywhere on the
/// path.
#[test]
fn test_read_skill_executor_refuses_well_shaped_path_outside_session_scope() {
    let temp_dir = TempDir::new().unwrap();
    // Real file, valid shape — the only thing wrong with it is that it is nowhere this
    // session could have indexed it from.
    let skill_path = create_test_skill_file(&temp_dir, "out-of-scope", "Not ours to read");

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        // No working directory set: nothing outside the home skill directories is in
        // scope, and the guard fails closed rather than reading from wherever the path
        // happens to land.
        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("out-of-scope-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(skill_path)),
            }),
            task_id: TaskId::new("out-of-scope-task".to_string()),
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
                    "A skill-shaped path outside the session's scope must be refused \
                     synchronously, with no disk read"
                ),
            }
        });
    });
}

/// The scope gate is lexical, so it constrains the *name*. A symlink planted at a
/// legitimate in-scope skill path still resolves elsewhere when the file is opened —
/// which is what turns "reads a SKILL.md in this session's tree" into an arbitrary-file
/// read. The read path re-checks the symlink-resolved path and refuses.
#[cfg(unix)]
#[test]
fn test_read_skill_executor_refuses_symlink_escaping_session_scope() {
    let temp_dir = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let secret = elsewhere.path().join("private-key");
    fs::write(&secret, "not a skill, and none of the agent's business").unwrap();

    let skill_dir = temp_dir.path().join(".agents/skills/exfil");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_path = skill_dir.join("SKILL.md");
    std::os::unix::fs::symlink(&secret, &skill_path).unwrap();

    let working_directory = temp_dir.path().to_string_lossy().to_string();

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let active_session = build_local_active_session(&mut app);
        active_session.update(&mut app, |session, _ctx| {
            session.set_current_working_directory_for_test(working_directory);
        });
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("symlink-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::Path(LocalOrRemotePath::Local(skill_path)),
            }),
            task_id: TaskId::new("symlink-task".to_string()),
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

        // The lexical gate passes — the *name* is a legitimate in-scope skill path — so
        // this reaches the async read, where the resolved target is checked.
        let AnyActionExecution::Async {
            execute_future,
            on_complete,
        } = execution
        else {
            panic!("An in-scope skill path should reach the async read");
        };

        let async_result = execute_future.await;
        let result = app.update(|ctx| on_complete(async_result, ctx));

        match result {
            AIAgentActionResultType::ReadSkill(ReadSkillResult::Error(msg)) => {
                assert!(msg.starts_with("Skill not found"));
                assert!(
                    !msg.contains("none of the agent's business"),
                    "the symlink target's contents must never reach the result: {msg}"
                );
            }
            other => panic!("A symlink out of scope must be refused, got: {other:?}"),
        }
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
        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

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
        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));
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
/// This used to stand a synthetic skill id in for `warpctrl`, because the fork
/// shipped no `resources/bundled/skills/warpctrl` directory. That directory now
/// exists (#370), so the test uses the pin's id and name again. Like the pin, it
/// registers the skill through `add_bundled_skill_for_testing` rather than
/// reading the bundle off disk.
#[test]
fn test_read_skill_executor_rejects_warp_control_bundled_skills_when_disabled() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
        let _warp_control_cli = FeatureFlag::WarpControlCli.override_enabled(false);
        let skill_id = "warpctrl";
        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            manager.add_bundled_skill_for_testing(
                skill_id,
                bundled_skill(skill_id),
                BundledSkillActivation::RequiresFeature(FeatureFlag::WarpControlCli),
            );
        });
        let active_session = build_local_active_session(&mut app);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));
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

/// Defect-fix regression test (found by the app/ai pin-test sweep,
/// `docs/sweep/app-ai.md`): `ReadSkillExecutor` used to hard-code
/// `SkillPathOrigin::Local` regardless of the active session, so a
/// `BundledSkillId` read from a warpified-remote (SSH) session silently
/// resolved against the *client's* local bundled-skill catalog instead of the
/// connected host's.
///
/// Registers the same skill id with different content in the local catalog
/// and in a specific remote host's catalog, then reads it from a session
/// connected to that remote host. If the session/host_id plumbing regresses
/// back to always-Local, this fails by returning the local content (or a
/// "not found" error, since the pre-fix code never even looked at the remote
/// catalog) instead of the remote host's.
#[test]
fn test_read_skill_executor_resolves_bundled_skill_from_remote_session_host() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);

        let core_host_id = warp_core::HostId::new("remote-host-1".to_owned());
        let util_host_id = crate::code::buffer_location::core_host_id_to_util(&core_host_id);

        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            // Decoy: same skill id in the LOCAL catalog, with different content.
            // A regression back to hard-coded `SkillPathOrigin::Local` would read
            // this instead of the remote catalog below.
            manager.add_bundled_skill_for_testing(
                "shared-skill-id",
                bundled_skill_with_content("shared-skill-id", "local catalog content"),
                BundledSkillActivation::Always,
            );
            manager.add_remote_bundled_skill_for_testing(
                util_host_id,
                "shared-skill-id",
                bundled_skill_with_content("shared-skill-id", "remote host catalog content"),
                BundledSkillActivation::Always,
            );
        });

        let active_session = build_remote_active_session(&mut app, core_host_id);
        let executor_handle = app.add_model(|_| ReadSkillExecutor::new(active_session));

        let action = AIAgentAction {
            id: AIAgentActionId::from("remote-skill-action".to_string()),
            action: AIAgentActionType::ReadSkill(ReadSkillRequest {
                skill: SkillReference::BundledSkillId("shared-skill-id".to_string()),
            }),
            task_id: TaskId::new("remote-skill-task".to_string()),
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
                    let body = match &content.content {
                        AnyFileContent::StringContent(s) => s.clone(),
                        AnyFileContent::BinaryContent(_) => {
                            panic!("bundled skill content should be text")
                        }
                    };
                    assert!(
                        body.contains("remote host catalog content"),
                        "expected the remote host's catalog content, got: {body}"
                    );
                }
                other => panic!(
                    "Remote session should resolve the skill via the remote host's catalog, \
                     got: {other:?}"
                ),
            }
        });
    });
}
