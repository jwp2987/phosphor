//! Session-scoped store of applied file-edit diffs, so `/rewind` can restore
//! files to their pre-edit content.
//!
//! When a `RequestFileEdits` action's diffs resolve, [`crate::tui_file_edits_view`]
//! records them here keyed by `(conversation_id, action_id)`. On `/rewind`, the
//! session view looks up the diffs for the actions being truncated and writes
//! each diff's base (pre-edit) content back through `FileModel`
//! (see [`crate::tui_diff_storage::revert_file_diffs`]).
//!
//! **Limitation (inherited from how the app works):** only edits made in the
//! current TUI session are here. A *restored* conversation's edits carry only a
//! lossy diff with no real base content (see the app's
//! `convert_file_edits_to_file_diffs`), so — exactly as in the GUI — they cannot
//! be reverted. `/rewind` still truncates the conversation for those; it just
//! can't undo their file changes.

use std::collections::HashMap;

use warp::tui_export::{AIAgentActionId, AIConversationId, FileDiff};
use warpui::{AppContext, Entity, SingletonEntity};

/// Applied file-edit diffs recorded during this session, keyed by conversation
/// then action.
pub(crate) struct TuiFileEditRevertRegistry {
    by_conversation: HashMap<AIConversationId, HashMap<AIAgentActionId, Vec<FileDiff>>>,
}

impl TuiFileEditRevertRegistry {
    /// Registers the singleton. Called once at TUI startup.
    pub(crate) fn register(ctx: &mut AppContext) {
        ctx.add_singleton_model(|_| Self {
            by_conversation: HashMap::new(),
        });
    }

    /// Records the diffs applied by `action_id` in `conversation_id`. No-op for
    /// an empty diff set (e.g. a no-op edit).
    pub(crate) fn record(
        &mut self,
        conversation_id: AIConversationId,
        action_id: AIAgentActionId,
        diffs: Vec<FileDiff>,
    ) {
        if diffs.is_empty() {
            return;
        }
        self.by_conversation
            .entry(conversation_id)
            .or_default()
            .insert(action_id, diffs);
    }

    /// The action ids with recorded diffs for `conversation_id`.
    pub(crate) fn action_ids_for(
        &self,
        conversation_id: &AIConversationId,
    ) -> Vec<AIAgentActionId> {
        self.by_conversation
            .get(conversation_id)
            .map(|actions| actions.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Removes and returns the recorded diffs for `action_id`, if any.
    pub(crate) fn take_diffs(
        &mut self,
        conversation_id: &AIConversationId,
        action_id: &AIAgentActionId,
    ) -> Option<Vec<FileDiff>> {
        self.by_conversation
            .get_mut(conversation_id)?
            .remove(action_id)
    }

    /// Drops all recorded diffs for `conversation_id` (e.g. when it is deleted).
    pub(crate) fn forget_conversation(&mut self, conversation_id: &AIConversationId) {
        self.by_conversation.remove(conversation_id);
    }
}

impl Entity for TuiFileEditRevertRegistry {
    type Event = ();
}

impl SingletonEntity for TuiFileEditRevertRegistry {}
