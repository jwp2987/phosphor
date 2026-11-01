use super::*;
use crate::ai::agent::AIAgentActionId;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::task::TaskId;
use warpui::App;

fn suggestion_action() -> AIAgentAction {
    AIAgentAction {
        id: AIAgentActionId::from("suggest".to_owned()),
        action: AIAgentActionType::SuggestNewConversation {
            message_id: "next-step".to_owned(),
        },
        task_id: TaskId::new("task".to_owned()),
        requires_result: true,
    }
}

/// The UIs queue the user's Accept/Reject and only then execute the action. Execution must keep
/// that decision instead of replacing it with the fast-fail Reject. Fails on the code that
/// drained the channel first: the Accept was dropped and the tool call came back Rejected.
#[test]
fn execute_keeps_a_decision_queued_before_it() {
    App::test((), |mut app| async move {
        let executor = app.add_model(|_| SuggestNewConversationExecutor::new());
        let action = suggestion_action();
        let queued = executor.update(&mut app, |executor, ctx| {
            executor.complete_suggest_new_conversation_action(NewConversationDecision::Accept);
            let _ = executor.execute(
                ExecuteActionInput {
                    action: &action,
                    conversation_id: AIConversationId::new(),
                },
                ctx,
            );
            let rx = &executor.suggest_new_conversation_result_rx.1;
            (rx.len(), rx.try_recv())
        });
        assert_eq!(
            queued.0, 1,
            "no fast-fail Reject on top of the user's decision"
        );
        assert!(matches!(queued.1, Ok(NewConversationDecision::Accept)));
    });
}

/// With no decision queued the fast-fail Reject is still sent, so the call cannot hang.
#[test]
fn execute_without_a_decision_queues_the_fast_fail_reject() {
    App::test((), |mut app| async move {
        let executor = app.add_model(|_| SuggestNewConversationExecutor::new());
        let action = suggestion_action();
        let queued = executor.update(&mut app, |executor, ctx| {
            let _ = executor.execute(
                ExecuteActionInput {
                    action: &action,
                    conversation_id: AIConversationId::new(),
                },
                ctx,
            );
            let rx = &executor.suggest_new_conversation_result_rx.1;
            (rx.len(), rx.try_recv())
        });
        assert_eq!(queued.0, 1);
        assert!(matches!(queued.1, Ok(NewConversationDecision::Reject)));
    });
}
