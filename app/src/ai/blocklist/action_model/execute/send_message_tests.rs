use super::*;
use crate::ai::agent::task::TaskId;
use crate::ai::agent::{
    AIAgentAction, AIAgentActionId, AIAgentActionResultType, AIAgentActionType,
};
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::blocklist::BlocklistAIHistoryModel;
use ai::agent::action_result::SendMessageToAgentResult;
use warpui::{App, EntityId, ModelHandle};

fn build_action(addresses: Vec<String>, subject: &str, message: &str) -> AIAgentAction {
    AIAgentAction {
        id: AIAgentActionId::from("send-message-1".to_string()),
        action: AIAgentActionType::SendMessageToAgent {
            addresses,
            subject: subject.to_string(),
            message: message.to_string(),
        },
        task_id: TaskId::new("task-send-message-1".to_string()),
        requires_result: false,
    }
}

/// Sets up an isolated on-disk mailbox root for the duration of `body`,
/// serialized against other tests via `#[serial]` since the mailbox root is
/// selected through a process-global env var
/// (`warp_cli::agent_mailbox::AGENT_MAILBOX_ROOT_ENV`). `body` receives an
/// owned `PathBuf` (rather than a borrow into this function's stack frame)
/// so it stays valid inside `App::test`'s `'static` future.
fn with_isolated_mailbox<T>(body: impl FnOnce(std::path::PathBuf) -> T) -> T {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: serialized via `#[serial(send_message_executor_mailbox)]` on every
    // caller, so no other test observes a partial env mutation.
    unsafe {
        std::env::set_var(warp_cli::agent_mailbox::AGENT_MAILBOX_ROOT_ENV, dir.path());
    }
    let result = body(dir.path().to_path_buf());
    unsafe {
        std::env::remove_var(warp_cli::agent_mailbox::AGENT_MAILBOX_ROOT_ENV);
    }
    result
}

fn initialize_history(app: &mut App) -> ModelHandle<BlocklistAIHistoryModel> {
    app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test())
}

#[test]
#[serial_test::serial(send_message_executor_mailbox)]
fn execute_delivers_to_the_target_mailbox_and_reports_success() {
    with_isolated_mailbox(|mailbox_root| {
        App::test((), |mut app| async move {
            let terminal_view_id = EntityId::new();
            let history = initialize_history(&mut app);
            let executor = app.add_model(|_| SendMessageToAgentExecutor::new());

            let conversation_id = history.update(&mut app, |history, ctx| {
                let conversation_id =
                    history.start_new_conversation(terminal_view_id, false, false, ctx);
                history.assign_run_id_for_conversation(
                    conversation_id,
                    "sender-run-1".to_string(),
                    None,
                    terminal_view_id,
                    ctx,
                );
                conversation_id
            });

            let action = build_action(vec!["target-run-1".to_string()], "status", "starting up");
            let execution = executor.update(&mut app, |executor, ctx| {
                let input = ExecuteActionInput {
                    action: &action,
                    conversation_id,
                };
                executor.execute(input, ctx)
            });

            let ActionExecution::Sync(AIAgentActionResultType::SendMessageToAgent(result)) =
                execution
            else {
                panic!("Expected a synchronous SendMessageToAgent result");
            };
            let SendMessageToAgentResult::Success { message_id } = result else {
                panic!("Expected Success, got {result:?}");
            };
            assert!(!message_id.is_empty());

            let delivered =
                warp_cli::agent_mailbox::list_messages(&mailbox_root, "target-run-1", 10).unwrap();
            assert_eq!(delivered.len(), 1);
            assert_eq!(delivered[0].from, "sender-run-1");
            assert_eq!(delivered[0].to, "target-run-1");
            assert_eq!(delivered[0].subject, "status");
            assert_eq!(delivered[0].body, "starting up");
            assert_eq!(delivered[0].message_id, message_id);
        });
    });
}

#[test]
#[serial_test::serial(send_message_executor_mailbox)]
fn execute_falls_back_to_the_ambient_task_id_when_the_conversation_has_no_run_id() {
    with_isolated_mailbox(|mailbox_root| {
        App::test((), |mut app| async move {
            let terminal_view_id = EntityId::new();
            let history = initialize_history(&mut app);
            let executor = app.add_model(|_| SendMessageToAgentExecutor::new());

            let conversation_id = history.update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });

            let ambient_task_id: AmbientAgentTaskId =
                "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
            executor.update(&mut app, |executor, _ctx| {
                executor.set_ambient_agent_task_id(Some(ambient_task_id));
            });

            let action = build_action(vec!["target-run-1".to_string()], "status", "body");
            executor.update(&mut app, |executor, ctx| {
                let input = ExecuteActionInput {
                    action: &action,
                    conversation_id,
                };
                executor.execute(input, ctx)
            });

            let delivered =
                warp_cli::agent_mailbox::list_messages(&mailbox_root, "target-run-1", 10).unwrap();
            assert_eq!(delivered.len(), 1);
            assert_eq!(delivered[0].from, ambient_task_id.to_string());
        });
    });
}

#[test]
#[serial_test::serial(send_message_executor_mailbox)]
fn execute_returns_error_and_sends_no_mail_when_addresses_are_empty() {
    with_isolated_mailbox(|_mailbox_root| {
        App::test((), |mut app| async move {
            let terminal_view_id = EntityId::new();
            let history = initialize_history(&mut app);
            let executor = app.add_model(|_| SendMessageToAgentExecutor::new());

            let conversation_id = history.update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });

            let action = build_action(vec![], "status", "body");
            let execution = executor.update(&mut app, |executor, ctx| {
                let input = ExecuteActionInput {
                    action: &action,
                    conversation_id,
                };
                executor.execute(input, ctx)
            });

            let ActionExecution::Sync(AIAgentActionResultType::SendMessageToAgent(
                SendMessageToAgentResult::Error(_),
            )) = execution
            else {
                panic!("Expected a synchronous SendMessageToAgent Error result");
            };
        });
    });
}

#[test]
fn should_autoexecute_is_always_true() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let history = initialize_history(&mut app);
        let executor = app.add_model(|_| SendMessageToAgentExecutor::new());

        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        let action = build_action(vec!["target-run-1".to_string()], "status", "body");
        let result = executor.update(&mut app, |executor, ctx| {
            let input = ExecuteActionInput {
                action: &action,
                conversation_id,
            };
            executor.should_autoexecute(input, ctx)
        });

        assert!(result);
    });
}
