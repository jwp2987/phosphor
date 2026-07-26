use crate::cloud_object::ObjectType;
use crate::code_review::diff_state::DiffMode;
use crate::search::mixer::SearchMixer;

pub type AIContextMenuMixer = SearchMixer<AIContextMenuSearchableAction>;

#[derive(Debug, Clone, PartialEq)]
pub enum AIContextMenuSearchableAction {
    InsertFilePath {
        /// This is the file path relative to the root of the current git
        /// repository. If this changes, this could break how we resolve
        /// the file path outside of AI mode, so just note the downstream
        /// dependencies.
        file_path: String,
    },
    InsertText {
        /// Text to insert into the input buffer.
        text: String,
    },
    InsertDriveObject {
        /// The Drive object type (Workflow, Notebook, etc.).
        object_type: ObjectType,
        /// The UID of the Drive object to attach.
        object_uid: String,
        /// The @name shown in the Agent Mode input box.
        display_name: String,
    },
    InsertPlan {
        /// The UID of the AI document to attach.
        ai_document_uid: String,
        /// The @name shown in the Agent Mode input box.
        display_name: String,
    },
    InsertDiffSet {
        /// The diff mode indicating what base to compare against
        diff_mode: DiffMode,
    },
    InsertConversation {
        /// The conversation identifier to attach.
        conversation_id: String,
        /// The @title shown in the Agent Mode input box.
        title: String,
    },
    InsertSkill {
        /// The skill name to insert as /{name} into the buffer.
        name: String,
    },
}
