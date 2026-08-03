use warpui::App;

use super::*;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::task::TaskId;
use crate::ai::agent::{
    AIAgentAction, AIAgentActionId, AIAgentActionResultType, AIAgentActionType,
    ReadDocumentsRequest, ReadDocumentsResult,
};
use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::ai::document::ai_document_model::{AIDocumentId, AIDocumentModel};
use crate::appearance::Appearance;
use crate::test_util::settings::initialize_settings_for_tests;

fn initialize_app(app: &mut App) {
    initialize_settings_for_tests(app);
    app.add_singleton_model(|_| Appearance::mock());
    app.add_singleton_model(|_| AIDocumentModel::new_for_test());
    app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
}

fn read_action(document_id: AIDocumentId) -> AIAgentAction {
    AIAgentAction {
        id: AIAgentActionId::from("read-documents-action".to_string()),
        task_id: TaskId::new("read-documents-task".to_string()),
        requires_result: true,
        action: AIAgentActionType::ReadDocuments(ReadDocumentsRequest {
            document_ids: vec![document_id],
        }),
    }
}

// NOTE: Warp's `execute_lazily_hydrates_missing_plan_for_remote_child_without_local_parent`
// is not ported here — it exercises a cloud/collab feature (`CloudNotebook` / `notebooks`
// lazily-fetching a saved plan for a remote-parented child conversation) that does not exist
// in this fork. See the PORT task report for the NEEDS-ADAPTATION breakdown.

#[test]
fn execute_returns_error_for_missing_document_id() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let executor = app.add_model(|_| ReadDocumentsExecutor::new());
        let missing_document_id = AIDocumentId::new();
        let action = read_action(missing_document_id);

        let execution: AnyActionExecution = executor.update(&mut app, |executor, ctx| {
            executor
                .execute(
                    ExecuteActionInput {
                        action: &action,
                        conversation_id: AIConversationId::new(),
                    },
                    ctx,
                )
                .into()
        });

        let AnyActionExecution::Sync(AIAgentActionResultType::ReadDocuments(
            ReadDocumentsResult::Error(error),
        )) = execution
        else {
            panic!("expected read_documents error");
        };
        assert!(error.contains(&missing_document_id.to_string()));
    });
}
