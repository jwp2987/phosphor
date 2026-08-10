use futures::{FutureExt, future::BoxFuture};
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::ai::{
    agent::{
        AIAgentAction, AIAgentActionType, DocumentContext, ReadDocumentsRequest,
        ReadDocumentsResult,
    },
    document::ai_document_model::AIDocumentModel,
};

use super::{ActionExecution, AnyActionExecution, ExecuteActionInput, PreprocessActionInput};

pub struct ReadDocumentsExecutor;

impl ReadDocumentsExecutor {
    pub fn new() -> Self {
        Self
    }

    pub(super) fn should_autoexecute(
        &self,
        _input: ExecuteActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> bool {
        // Document operations are always auto-executed
        true
    }

    pub(super) fn execute(
        &mut self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> impl Into<AnyActionExecution> + use<> {
        let ExecuteActionInput { action, .. } = input;
        let AIAgentAction {
            action: AIAgentActionType::ReadDocuments(ReadDocumentsRequest { document_ids }),
            ..
        } = action
        else {
            return ActionExecution::<ReadDocumentsResult>::InvalidAction;
        };

        // Access the model synchronously before the async block. Any requested
        // document id that isn't loaded locally is reported as an error, matching
        // Warp's behavior — silently succeeding with the document omitted would
        // hide the failure from the LLM.
        let model = AIDocumentModel::handle(ctx);
        let mut documents = Vec::with_capacity(document_ids.len());
        let mut missing_documents = Vec::new();
        for id in document_ids.iter() {
            let document = {
                let model = model.as_ref(ctx);
                model.get_document_content(id, ctx).and_then(|content| {
                    let version = model.get_current_document(id)?.version;
                    Some(DocumentContext {
                        document_id: *id,
                        content,
                        line_ranges: vec![],
                        document_version: version,
                    })
                })
            };
            match document {
                Some(document) => documents.push(document),
                None => missing_documents.push(*id),
            }
        }

        if !missing_documents.is_empty() {
            let missing_list = missing_documents
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return ActionExecution::Sync(
                ReadDocumentsResult::Error(format!("Document(s) not found: {missing_list}")).into(),
            );
        }

        ActionExecution::Sync(ReadDocumentsResult::Success { documents }.into())
    }

    pub(super) fn preprocess_action(
        &mut self,
        _input: PreprocessActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        futures::future::ready(()).boxed()
    }
}

impl Entity for ReadDocumentsExecutor {
    type Event = ();
}

#[cfg(test)]
#[path = "read_documents_test.rs"]
mod tests;
