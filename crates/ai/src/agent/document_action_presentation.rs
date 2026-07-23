use crate::agent::action::AIAgentActionType;
use crate::agent::action_result::{
    AIAgentActionResultType, CreateDocumentsResult, EditDocumentsResult,
};
use crate::document::{AIDocumentId, AIDocumentVersion, DEFAULT_PLANNING_DOCUMENT_TITLE};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCallDisplayState {
    Constructing,
    Pending,
    AwaitingApproval,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentedDocument {
    pub title: String,
    pub content: String,
    pub document_id: Option<AIDocumentId>,
    pub document_version: Option<AIDocumentVersion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentActionPresentation {
    pub documents: Vec<PresentedDocument>,
}

impl DocumentActionPresentation {
    pub fn resolve(
        action: &AIAgentActionType,
        result: Option<&AIAgentActionResultType>,
    ) -> Option<Self> {
        match (action, result) {
            (
                AIAgentActionType::CreateDocuments(request),
                Some(AIAgentActionResultType::CreateDocuments(CreateDocumentsResult::Success {
                    created_documents,
                })),
            ) => Some(Self {
                documents: created_documents
                    .iter()
                    .enumerate()
                    .map(|(index, document)| PresentedDocument {
                        title: request
                            .documents
                            .get(index)
                            .map(|document| document.title.as_str())
                            .filter(|title| !title.is_empty())
                            .map(str::to_owned)
                            .unwrap_or_else(|| document_title(index, created_documents.len())),
                        content: document.content.clone(),
                        document_id: Some(document.document_id),
                        document_version: Some(document.document_version),
                    })
                    .collect(),
            }),
            (AIAgentActionType::CreateDocuments(_), Some(_)) => Some(Self {
                documents: Vec::new(),
            }),
            (AIAgentActionType::CreateDocuments(request), None) => Some(Self {
                documents: request
                    .documents
                    .iter()
                    .enumerate()
                    .map(|(index, document)| PresentedDocument {
                        title: if document.title.is_empty() {
                            document_title(index, request.documents.len())
                        } else {
                            document.title.clone()
                        },
                        content: document.content.clone(),
                        document_id: None,
                        document_version: None,
                    })
                    .collect(),
            }),
            (
                AIAgentActionType::EditDocuments(_),
                Some(AIAgentActionResultType::EditDocuments(EditDocumentsResult::Success {
                    updated_documents,
                })),
            ) => Some(Self {
                documents: updated_documents
                    .iter()
                    .enumerate()
                    .map(|(index, document)| PresentedDocument {
                        title: document_title(index, updated_documents.len()),
                        content: document.content.clone(),
                        document_id: Some(document.document_id),
                        document_version: Some(document.document_version),
                    })
                    .collect(),
            }),
            (AIAgentActionType::EditDocuments(_), Some(_) | None) => Some(Self {
                documents: Vec::new(),
            }),
            // Any non-document action: no planning-document presentation. Kept
            // as a wildcard (rather than upstream's explicit enumeration) so it
            // stays exhaustive over Zap's subset of AIAgentActionType.
            (_, _) => None,
        }
    }
}

fn document_title(index: usize, document_count: usize) -> String {
    if document_count == 1 {
        DEFAULT_PLANNING_DOCUMENT_TITLE.to_owned()
    } else {
        format!("Document {}", index + 1)
    }
}

