use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use ai::skills::SkillPathOrigin;
use anyhow::anyhow;
use chrono::{DateTime, Local, NaiveDateTime};
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warp_cli::agent::Harness;
use warp_multi_agent_api::response_event::stream_finished::ConversationUsageMetadata;
use warp_multi_agent_api::{
    client_action::{Action, StartNewConversation},
    response_event::stream_finished::TokenUsage,
};
use warpui::{AppContext, Entity, EntityId, ModelContext, SingletonEntity};

#[cfg(feature = "local_fs")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "local_fs")]
use diesel::SqliteConnection;

use crate::ai::agent::api::ServerConversationToken;
use crate::ai::agent::conversation::ConversationStatus;
use crate::ai::agent::conversation::UpdateConversationError;
use crate::ai::agent::task::helper::{MessageExt, ToolCallExt};
use crate::ai::agent::task::TaskId;
use crate::ai::agent::AIAgentExchangeId;
use crate::ai::agent::CancellationReason;
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::artifacts::Artifact;
use crate::ai::byop_readiness::{
    RepairRecord, RepairSource, RepairState, RepairStateStatus, ToolCallKey,
};
use crate::ai::document::ai_document_model::AIDocumentModel;
use crate::input_suggestions::HistoryOrder;
use crate::persistence::model::AgentConversationData;
use crate::persistence::ModelEvent;
use crate::terminal::model::block::BlockId;
use crate::terminal::view::blocklist_filter;
use crate::GlobalResourceHandlesProvider;
use crate::{
    ai::agent::{
        conversation::{AIConversation, AIConversationId},
        AIAgentActionId, AIAgentExchange, AIAgentInput, AIAgentOutputStatus, FinishedAIAgentOutput,
        MessageId, RenderableAIError, RequestCost, Suggestions,
    },
    persistence::model::AgentConversation,
    ui_components::icons::Icon,
};

#[cfg(feature = "local_fs")]
use crate::persistence::{database_file_path, establish_ro_connection};

use super::controller::response_stream::ResponseStreamId;
use super::persistence::{PersistedAIInput, PersistedAIInputType};
use super::RequestInput;

mod conversation_loader;
pub use conversation_loader::{
    convert_persisted_conversation_to_ai_conversation_with_metadata, CLIAgentConversation,
    LoadedConversationData,
};

pub(super) const MAX_HISTORICAL_CONVERSATIONS: usize = 100;

/// Metadata for conversations restored from memory or local SQLite.
#[derive(Debug, Clone)]
pub struct AIConversationMetadata {
    pub id: AIConversationId,

    pub title: String,

    pub initial_query: String,

    pub last_modified_at: NaiveDateTime,

    pub initial_working_directory: Option<String>,

    pub credits_spent: Option<f32>,

    pub server_conversation_token: Option<ServerConversationToken>,

    /// Whether the full conversation data exists in the local database.
    pub has_local_data: bool,

    /// Artifacts (plans, PRs) created during this conversation.
    pub artifacts: Vec<Artifact>,

    /// Local marker for conversations owned by an ambient agent run.
    pub ambient_agent_task_id: Option<AmbientAgentTaskId>,

    /// Local conversation ID of the parent that spawned this child, if any.
    /// Mirrors [`AIConversation::parent_conversation_id`]; stored here so
    /// child-agent status can be determined from metadata alone, without
    /// loading the full conversation. See [`Self::is_child_agent_conversation`].
    pub parent_conversation_id: Option<AIConversationId>,

    /// Server-side parent agent identifier (the parent's run_id), if any.
    /// Mirrors [`AIConversation::parent_agent_id`]; the same value the ambient
    /// task carries as `parent_run_id`.
    pub parent_agent_id: Option<String>,
}

impl From<&AIConversation> for AIConversationMetadata {
    fn from(conversation: &AIConversation) -> Self {
        let title = conversation.title().unwrap_or_default().to_string();
        let initial_query: String = conversation.initial_query().unwrap_or_default();

        let last_modified_at = conversation
            .latest_exchange()
            .map(|exchange| exchange.start_time.naive_utc())
            .unwrap_or_else(|| chrono::Utc::now().naive_utc());

        Self {
            id: conversation.id(),
            title,
            initial_query,
            last_modified_at,
            initial_working_directory: conversation.initial_working_directory(),
            credits_spent: Some(conversation.credits_spent()),
            server_conversation_token: conversation.server_conversation_token().cloned(),
            has_local_data: true,
            artifacts: conversation.artifacts().to_vec(),
            ambient_agent_task_id: conversation.task_id(),
            parent_conversation_id: conversation.parent_conversation_id(),
            parent_agent_id: conversation.parent_agent_id().map(ToString::to_string),
        }
    }
}

impl AIConversationMetadata {
    /// Whether this conversation is owned by an ambient agent run rather than
    /// being a direct user conversation.
    pub fn is_ambient_agent_conversation(&self) -> bool {
        self.ambient_agent_task_id.is_some()
    }

    /// Whether this conversation was spawned by a parent orchestrator agent.
    /// Uses the same predicate as [`AIConversation::is_child_agent_conversation`]
    /// so the loaded and unloaded representations agree.
    pub fn is_child_agent_conversation(&self) -> bool {
        self.parent_conversation_id.is_some() || self.parent_agent_id.is_some()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateHistoryError {
    #[error("Failed to update conversation: {0:?}")]
    Conversation(#[from] UpdateConversationError),
    #[error("Failed to find conversation with ID {0:?}")]
    ConversationNotFound(AIConversationId),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ForkConversationError {
    #[error("cannot fork an empty conversation")]
    EmptyConversation,
}

/// Reasons an optimistic conversation rename cannot begin.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BeginConversationRenameError {
    #[error("a rename is already in flight for this conversation")]
    RenameInProgress,
    #[error("conversation not found")]
    ConversationNotFound,
    #[error("conversation has no server conversation token")]
    MissingServerConversationToken,
    #[error("conversation root task is not ready to be renamed")]
    ConversationNotReady,
}

/// Rollback snapshot captured while an optimistic rename is in flight.
///
/// The title is applied locally immediately so the UI reflects the change
/// without waiting; if the rename request fails, the previous title is
/// restored from this snapshot.
#[derive(Debug, Clone)]
struct InFlightConversationRename {
    /// The title the user attempted to apply. Used to decide whether a
    /// server-normalized title on completion differs and must be re-applied.
    attempted_title: String,
    /// Previous root task description (the conversation title source of truth).
    previous_root_task_description: String,
    /// Previous cached metadata title, if the conversation had cached metadata.
    previous_cached_metadata_title: Option<String>,
}

/// A single agent prompt-history candidate with prompt text and start_ts.
///
/// Ported from the pin (`app/src/ai/blocklist/history_model.rs:236-241`, `02b53fcd8`) for #256.
#[derive(Clone, Debug)]
pub(crate) struct PromptHistoryEntry {
    /// The user prompt text.
    pub(crate) text: Arc<str>,
    /// When the prompt was submitted.
    pub(crate) start_ts: DateTime<Local>,
}

/// Responsible for managing the history of user and AI exchanges.
#[derive(Default)]
pub struct BlocklistAIHistoryModel {
    /// A [`HashMap`] mapping [`crate::terminal::TerminalView`] [`EntityId`]s to a [`Vec`] of
    /// live [`AIConversationId`] in that `TerminalView`.
    ///
    /// "Live" conversations are still visible and in the terminal view and selectable in the session, so
    /// clearing the blocklist removes the conversation from here.
    ///
    /// Note that when a terminal view is closed, we do not remove it from this map, so that it can be restored.
    live_conversation_ids_for_terminal_view: HashMap<EntityId, Vec<AIConversationId>>,

    /// A [`HashMap`] mapping [`crate::terminal::TerminalView`] [`EntityId`]s to a [`Vec`] of
    /// [`AIConversationId`] that were once live in that session, but were cleared from the blocklist.
    ///
    /// This is used to preserve queries for up-arrow history after clearing the blocklist.
    cleared_conversation_ids_for_terminal_view: HashMap<EntityId, Vec<AIConversationId>>,

    /// A [`HashMap`] mapping a [`AIConversationId`] to the [`AIConversation`] itself.
    /// Conversations may or may not be live in any open session. They will exist in this map if they
    /// have ever been loaded into memory.
    conversations_by_id: HashMap<AIConversationId, AIConversation>,

    /// The active conversation ID for a given terminal view.
    /// The active conversation is the one we're currently or have most recently streamed outputs for.
    /// If you want to get the conversation the next query will follow up in / what is selected in the input selector,
    /// use `context_model.selected_conversation_id` instead.
    active_conversation_for_terminal_view: HashMap<EntityId, AIConversationId>,

    /// The time at which each [`TerminalView`] was created. Note that this has no bearing on when
    /// any [`AIConversation`]s take place in the terminal view.
    terminal_view_created_at: HashMap<EntityId, DateTime<Local>>,

    /// A set of terminal views that are shared ambient agent sessions.
    ambient_agent_terminal_view_ids: HashSet<EntityId>,

    /// A set of terminal views that are read-only conversation transcript viewers.
    /// This is view/UI state (not conversation state) and is used to filter transcript viewer
    /// conversations out of local history and navigation.
    conversation_transcript_viewer_terminal_view_ids: HashSet<EntityId>,

    /// AI queries that were read from the SQLite DB. These exchanges do not contain as much
    /// information as the other exchanges we store because they are only used for display in
    /// history.
    persisted_queries: Vec<PersistedAIInput>,

    /// Agent prompt-history candidates for NLD input classification, ascending
    /// (oldest-first). Seeded from a persisted snapshot in `new` (#256 item 2) and
    /// consumed by `input_model.rs`'s `resolve_history_match` (#312). The live-session
    /// append path and the SQLite-backed snapshot source are tracked separately
    /// (superseded by #336/#337/#331), so the snapshot is currently always empty.
    prompt_history: Vec<PromptHistoryEntry>,

    /// Metadata for both local and ambient agent conversations.
    /// Does not include the actual content of the conversations.
    all_conversations_metadata: HashMap<AIConversationId, AIConversationMetadata>,

    /// Reverse index from server-side agent identifier to local conversation ID.
    ///
    /// Keyed by the conversation run id when available, falling back to the
    /// server conversation token for restored legacy conversations.
    agent_id_to_conversation_id: HashMap<String, AIConversationId>,

    /// Reverse index from [`ServerConversationToken`] to local [`AIConversationId`].
    ///
    /// Maintained alongside every mutation of `conversations_by_id` and
    /// `all_conversations_metadata` that involves a token. Used to make
    /// `find_conversation_id_by_server_token` O(1); it is called once per
    /// ambient-agent task on every conversation-list refresh.
    server_token_to_conversation_id: HashMap<ServerConversationToken, AIConversationId>,

    /// Index from parent conversation ID to child conversation IDs.
    /// Populated at startup from the local DB and kept in sync at runtime
    /// via `set_parent_for_conversation` and `restore_conversations`.
    children_by_parent: HashMap<AIConversationId, Vec<AIConversationId>>,

    /// Tombstone set of conversation ids explicitly killed via the
    /// orchestration pill bar's "Kill agent" action. Bounded LRU, like the
    /// pin's `OrchestrationEventStreamer::killed_run_ids` (which this
    /// fork does not have -- that streamer is cloud, see DECLINED.md).
    ///
    /// Local equivalent, scoped to what this fork can actually resurrect a
    /// killed conversation from: `start_new_child_conversation`, called when
    /// a local lifecycle/spawn event for the same child arrives after Kill
    /// already deleted it. Consulted there; not a full port of the pin's
    /// event-batch-level filtering (which guards a server-relay path this
    /// fork doesn't have).
    killed_conversation_ids: HashSet<AIConversationId>,
    /// Insertion order for `killed_conversation_ids`, so eviction is FIFO
    /// once the bound is hit.
    killed_conversation_id_order: std::collections::VecDeque<AIConversationId>,

    /// Optimistic conversation renames currently in flight, keyed by
    /// conversation ID. Present between `begin_conversation_rename` and the
    /// matching `complete_conversation_rename` / `fail_conversation_rename`.
    /// Holds the rollback snapshot used if the rename fails.
    in_flight_conversation_renames: HashMap<AIConversationId, InFlightConversationRename>,

    #[cfg(feature = "local_fs")]
    db_connection: Option<Arc<Mutex<SqliteConnection>>>,
}

impl BlocklistAIHistoryModel {
    pub(crate) fn new(
        persisted_queries: Vec<PersistedAIInput>,
        prompt_history: Vec<(String, DateTime<Local>)>,
        multi_agent_conversations: &[AgentConversation],
    ) -> Self {
        #[cfg(feature = "local_fs")]
        let db_connection = database_file_path().to_str().and_then(|db_url| {
            establish_ro_connection(db_url)
                .ok()
                .map(|conn| Arc::new(Mutex::new(conn)))
        });

        let prompt_history = prompt_history
            .into_iter()
            .filter(|(text, _)| !text.trim().is_empty())
            .map(|(text, start_ts)| PromptHistoryEntry {
                text: Arc::from(text),
                start_ts,
            })
            .collect();

        let mut model = Self {
            persisted_queries,
            prompt_history,
            #[cfg(feature = "local_fs")]
            db_connection,
            ..Self::default()
        };

        // Initialize historical conversations from local DB
        model.initialize_historical_conversations(multi_agent_conversations);

        model
    }

    #[cfg(test)]
    pub(crate) fn new_for_test() -> Self {
        Self::default()
    }

    /// Returns a flattened and ordered (oldest first) list of live conversations (not cleared) for the given terminal view ID.
    /// This works for terminal views that have been closed.
    pub fn all_live_conversations_for_terminal_view(
        &self,
        terminal_view_id: EntityId,
    ) -> impl Iterator<Item = &AIConversation> {
        self.live_conversation_ids_for_terminal_view
            .get(&terminal_view_id)
            .into_iter()
            .flat_map(|conversation_ids| {
                conversation_ids
                    .iter()
                    .filter_map(|conversation_id| self.conversation(conversation_id))
            })
    }

    /// Returns a flattened and ordered (oldest first) list of exchanges from live conversations (not cleared)
    /// in the given terminal view ID.
    /// This works for terminal views that have been closed.
    pub fn all_live_root_task_exchanges_for_terminal_view(
        &self,
        terminal_view_id: EntityId,
    ) -> impl Iterator<Item = &AIAgentExchange> {
        self.live_conversation_ids_for_terminal_view
            .get(&terminal_view_id)
            .into_iter()
            .flat_map(|conversation_ids| {
                conversation_ids.iter().flat_map(|conversation_id| {
                    self.conversations_by_id
                        .get(conversation_id)
                        .map(|conversation| conversation.root_task_exchanges())
                })
            })
            .flatten()
    }

    /// Returns a flattened and ordered (oldest first) list of exchanges from conversations
    /// that were cleared in the given terminal view ID, but are no longer live/visible.
    pub fn all_cleared_root_task_exchanges_for_terminal_view(
        &self,
        terminal_view_id: EntityId,
    ) -> impl Iterator<Item = &AIAgentExchange> {
        self.cleared_conversation_ids_for_terminal_view
            .get(&terminal_view_id)
            .into_iter()
            .flat_map(|conversation_ids| {
                conversation_ids.iter().flat_map(|conversation_id| {
                    self.conversations_by_id
                        .get(conversation_id)
                        .map(|conversation| conversation.root_task_exchanges())
                })
            })
            .flatten()
    }

    /// Returns the ids of every conversation cleared from `terminal_view_id`, or an empty
    /// slice if none. Used by `QueuedQueryModel` to drop the queues of conversations cleared
    /// via `ClearedConversationsInTerminalView` (whose event does not carry the ids).
    pub fn cleared_conversation_ids_for_terminal_view(
        &self,
        terminal_view_id: EntityId,
    ) -> &[AIConversationId] {
        self.cleared_conversation_ids_for_terminal_view
            .get(&terminal_view_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Returns a list of all conversations that have been cleared across all terminal views.
    pub fn all_cleared_conversations(&self) -> Vec<(EntityId, &AIConversation)> {
        self.cleared_conversation_ids_for_terminal_view
            .iter()
            .flat_map(|(terminal_view_id, conversation_ids)| {
                conversation_ids.iter().filter_map(|conversation_id| {
                    self.conversations_by_id
                        .get(conversation_id)
                        .map(|conversation| (*terminal_view_id, conversation))
                })
            })
            .collect::<Vec<_>>()
    }

    /// Returns a list of all live (not cleared) conversations across all terminal views,
    /// paired with the terminal view ID they belong to.
    /// This includes terminal views that have been closed.
    pub fn all_live_conversations(&self) -> Vec<(EntityId, &AIConversation)> {
        self.live_conversation_ids_for_terminal_view
            .iter()
            .flat_map(|(terminal_view_id, conversation_ids)| {
                conversation_ids.iter().filter_map(|conversation_id| {
                    self.conversations_by_id
                        .get(conversation_id)
                        .map(|conversation| (*terminal_view_id, conversation))
                })
            })
            .collect::<Vec<_>>()
    }

    /// Returns a conversation by ID by reading from memory. The conversation may not be available if:
    /// * The ID is invalid
    /// * The conversation has never been read into memory from db. Use load_conversation_from_db to handle reading from db.
    pub fn conversation(&self, conversation_id: &AIConversationId) -> Option<&AIConversation> {
        self.conversations_by_id.get(conversation_id)
    }

    /// Returns the render status of one todo item in the conversation's todo
    /// history — a narrow projection for consumers that don't need the whole
    /// `AIConversation`.
    pub fn todo_status(
        &self,
        conversation_id: &AIConversationId,
        todo_id: &crate::ai::agent::AIAgentTodoId,
    ) -> Option<crate::ai::agent::conversation::TodoStatus> {
        self.conversation(conversation_id)?.todo_status(todo_id)
    }

    /// Returns the conversation's active (most recent) todo list, if any — a
    /// narrow projection (see [`Self::todo_status`]).
    pub fn active_todo_list(
        &self,
        conversation_id: &AIConversationId,
    ) -> Option<&crate::ai::agent::todos::AIAgentTodoList> {
        self.conversation(conversation_id)?.active_todo_list()
    }

    pub fn conversation_mut(
        &mut self,
        conversation_id: &AIConversationId,
    ) -> Option<&mut AIConversation> {
        self.conversations_by_id.get_mut(conversation_id)
    }

    /// Returns all child conversations whose `parent_conversation_id` matches
    /// the given parent ID, using the `children_by_parent` index.
    pub fn child_conversations_of(&self, parent_id: AIConversationId) -> Vec<&AIConversation> {
        self.child_conversation_ids_of(&parent_id)
            .iter()
            .filter_map(|id| self.conversations_by_id.get(id))
            .collect()
    }

    /// Creates a new child agent conversation.
    /// Bound on `killed_conversation_ids`, mirroring the pin's
    /// `MAX_KILLED_RUN_IDS` on `OrchestrationEventStreamer` (not ported --
    /// see the field's doc comment).
    const MAX_KILLED_CONVERSATION_IDS: usize = 256;

    /// Tombstones `conversation_id` so a later local re-creation attempt for
    /// it (see `start_new_child_conversation`'s check) is rejected instead of
    /// silently resurrecting a conversation the user explicitly killed.
    pub(crate) fn mark_conversation_killed(&mut self, conversation_id: AIConversationId) {
        if self.killed_conversation_ids.insert(conversation_id) {
            self.killed_conversation_id_order.push_back(conversation_id);
        }
        while self.killed_conversation_id_order.len() > Self::MAX_KILLED_CONVERSATION_IDS {
            let Some(evicted) = self.killed_conversation_id_order.pop_front() else {
                break;
            };
            self.killed_conversation_ids.remove(&evicted);
        }
    }

    pub(crate) fn is_conversation_killed(&self, conversation_id: &AIConversationId) -> bool {
        self.killed_conversation_ids.contains(conversation_id)
    }

    pub fn start_new_child_conversation(
        &mut self,
        terminal_view_id: EntityId,
        name: String,
        parent_conversation_id: AIConversationId,
        orchestration_harness: Option<Harness>,
        ctx: &mut ModelContext<Self>,
    ) -> AIConversationId {
        let parent_agent_id = self
            .conversation(&parent_conversation_id)
            .and_then(|c| c.agent_link_id());
        if parent_agent_id.is_none() {
            log::warn!(
                "No agent identifier for parent conversation {parent_conversation_id:?}; \
                 child agent will not be linked to parent on the server."
            );
        }

        let auto_execute = true; // Child auto-executes by default.
        let conversation_id =
            self.start_new_conversation(terminal_view_id, auto_execute, false, ctx);
        {
            let conversation = self
                .conversation_mut(&conversation_id)
                .expect("Child conversation exists — was just created.");
            if let Some(id) = parent_agent_id {
                conversation.set_parent_agent_id(id);
            }
            conversation.set_agent_name(name);
            if let Some(harness) = orchestration_harness {
                conversation.set_orchestration_harness(harness);
            }
        }
        self.set_parent_for_conversation(conversation_id, parent_conversation_id);
        // Without this write the child exists only in memory: a child agent that
        // has not yet taken a turn is absent from SQLite and cannot be restored,
        // so its parent link is lost across a restart. The pin persists at the
        // end of this same function
        // (`42effe840:app/src/ai/blocklist/history_model.rs:577`). The `&mut`
        // borrow of the child above is already scoped to end before this call,
        // which takes `&mut self`.
        self.persist_conversation_state(conversation_id, ctx);
        conversation_id
    }

    /// Resolves the parent of `conversation` in the orchestration tree.
    ///
    /// Prefers the local `parent_conversation_id` placeholder (set by
    /// `set_parent_for_conversation` when this client spawned the child
    /// itself); falls back to resolving `parent_agent_id` against the
    /// `agent_id_to_conversation_id` index for children whose parent
    /// linkage was established via the agent identifier alone.
    pub fn resolved_parent_conversation_id_for_conversation(
        &self,
        conversation: &AIConversation,
    ) -> Option<AIConversationId> {
        self.resolved_parent_conversation_id_from_refs(
            conversation.parent_conversation_id(),
            conversation.parent_agent_id(),
        )
    }

    /// The startup counterpart of
    /// [`Self::resolved_parent_conversation_id_for_conversation`]: resolves the
    /// parent of a *persisted* row, which at startup has not been hydrated into
    /// an [`AIConversation`] yet. Same precedence — the stored
    /// `parent_conversation_id`, then `parent_agent_id` resolved against the
    /// `agent_id_to_conversation_id` index, which
    /// `initialize_historical_conversations` seeds for every restorable row in
    /// a first pass before calling this. Ported from
    /// `42effe840:app/src/ai/blocklist/history_model.rs:508`.
    fn resolved_parent_conversation_id_from_persisted_data(
        &self,
        conversation_data: &AgentConversationData,
    ) -> Option<AIConversationId> {
        let parent_conversation_id = conversation_data
            .parent_conversation_id
            .as_deref()
            .and_then(|id| AIConversationId::try_from(id.to_owned()).ok());
        self.resolved_parent_conversation_id_from_refs(
            parent_conversation_id,
            conversation_data.parent_agent_id.as_deref(),
        )
    }

    fn resolved_parent_conversation_id_from_refs(
        &self,
        parent_conversation_id: Option<AIConversationId>,
        parent_agent_id: Option<&str>,
    ) -> Option<AIConversationId> {
        parent_conversation_id.or_else(|| {
            parent_agent_id.and_then(|agent_id| self.conversation_id_for_agent_id(agent_id))
        })
    }

    /// Sets the parent conversation ID on a child conversation and updates
    /// the `children_by_parent` index.  All parent-child relationships should
    /// be established through this method so the index stays in sync.
    pub fn set_parent_for_conversation(
        &mut self,
        child_id: AIConversationId,
        parent_id: AIConversationId,
    ) {
        if let Some(conversation) = self.conversations_by_id.get_mut(&child_id) {
            conversation.set_parent_conversation_id(parent_id);
        }
        let children = self.children_by_parent.entry(parent_id).or_default();
        if !children.contains(&child_id) {
            children.push(child_id);
        }
    }

    /// Returns the child conversation IDs for a parent from the startup index.
    /// Unlike `child_conversations_of`, this works before children are loaded
    /// into `conversations_by_id`.
    pub fn child_conversation_ids_of(&self, parent_id: &AIConversationId) -> &[AIConversationId] {
        self.children_by_parent
            .get(parent_id)
            .map(|v| v.as_slice())
            .unwrap_or_default()
    }

    /// Writes the current in-memory state of a conversation to SQLite. Used by
    /// callers that mutate durable conversation fields (event sequence, pin
    /// state, renames) and need the change to survive a restart.
    fn persist_conversation_state(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) else {
            return;
        };
        conversation.write_updated_conversation_state(ctx);
    }

    /// Updates the persisted `last_event_sequence` for a conversation and
    /// writes the updated conversation state to SQLite. Used by the
    /// orchestration event poller after draining an event batch to keep the
    /// cursor durable across restarts.
    pub fn update_event_sequence(
        &mut self,
        conversation_id: AIConversationId,
        sequence: i64,
        ctx: &mut ModelContext<Self>,
    ) {
        {
            let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) else {
                return;
            };
            conversation.set_last_event_sequence(sequence);
        }
        self.persist_conversation_state(conversation_id, ctx);
    }

    /// Marks a conversation as a remote-child placeholder and persists the
    /// change.
    ///
    /// This fork has no remote-worker execution path (see
    /// [`AIConversation::is_remote_child`]'s doc comment), so nothing in
    /// production ever calls this — `is_remote_child` is permanently
    /// `false` in practice. It is kept so `has_local_orchestrated_children`'s
    /// local/remote distinction stays testable and the shape matches the
    /// pin, in case legacy/foreign data ever carries the flag set.
    pub fn mark_conversation_as_remote_child(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        {
            let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) else {
                return;
            };
            conversation.mark_as_remote_child();
        }
        self.persist_conversation_state(conversation_id, ctx);
    }

    /// Toggles the orchestration pill-bar pin state for a conversation and
    /// persists the change so it survives a restart. Orchestrator
    /// conversations (no parent) are not pinnable — pinning only applies to
    /// child agents in the pill bar.
    pub fn set_conversation_pinned(
        &mut self,
        conversation_id: AIConversationId,
        pinned: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        {
            let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) else {
                return;
            };
            conversation.set_pinned(pinned);
        }
        self.persist_conversation_state(conversation_id, ctx);
    }

    /// Copies the live conversation's durable identity fields back onto its
    /// cached [`AIConversationMetadata`] entry, so metadata-driven UI (the
    /// conversation list) doesn't keep serving a stale token after a rebind.
    ///
    /// Ported from the pin (`42effe840:app/src/ai/blocklist/history_model.rs:617`)
    /// minus its `has_cloud_data` / `server_conversation_metadata` writes --
    /// neither field exists on this fork's `AIConversationMetadata` (deliberately
    /// de-clouded), so there is nothing to mirror them onto.
    fn update_cached_metadata_for_conversation(&mut self, conversation_id: AIConversationId) {
        let Some(conversation) = self.conversations_by_id.get(&conversation_id) else {
            return;
        };
        let Some(metadata) = self.all_conversations_metadata.get_mut(&conversation_id) else {
            return;
        };

        metadata.server_conversation_token = conversation.server_conversation_token().cloned();
    }

    /// Starts an optimistic local rename and records rollback state.
    ///
    /// The new title is applied immediately (so the UI updates without waiting)
    /// and a rollback snapshot is captured. Returns the conversation's server
    /// conversation token, which the caller uses to identify the conversation
    /// to whatever rename backend is in use. Callers must follow up with
    /// [`Self::complete_conversation_rename`] or [`Self::fail_conversation_rename`].
    pub(crate) fn begin_conversation_rename(
        &mut self,
        conversation_id: AIConversationId,
        title: String,
        ctx: &mut ModelContext<Self>,
    ) -> Result<String, BeginConversationRenameError> {
        if self
            .in_flight_conversation_renames
            .contains_key(&conversation_id)
        {
            return Err(BeginConversationRenameError::RenameInProgress);
        }

        let conversation = self
            .conversations_by_id
            .get(&conversation_id)
            .ok_or(BeginConversationRenameError::ConversationNotFound)?;
        let server_conversation_token = conversation
            .server_conversation_token()
            .ok_or(BeginConversationRenameError::MissingServerConversationToken)?
            .as_str()
            .to_owned();
        let root_task = conversation
            .get_root_task()
            .ok_or(BeginConversationRenameError::ConversationNotReady)?;
        // An optimistic root task (no persisted source) has no real title to
        // rename yet; reject until the conversation has been established.
        if root_task.source().is_none() {
            return Err(BeginConversationRenameError::ConversationNotReady);
        }
        let previous_root_task_description = root_task.description().to_owned();
        let previous_cached_metadata_title = self
            .all_conversations_metadata
            .get(&conversation_id)
            .map(|metadata| metadata.title.clone());

        self.in_flight_conversation_renames.insert(
            conversation_id,
            InFlightConversationRename {
                attempted_title: title.clone(),
                previous_root_task_description,
                previous_cached_metadata_title,
            },
        );
        self.apply_conversation_title(conversation_id, title, ctx);
        Ok(server_conversation_token)
    }

    /// Completes an in-flight rename and applies any normalized title.
    pub(crate) fn complete_conversation_rename(
        &mut self,
        conversation_id: AIConversationId,
        title: String,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(rename) = self.in_flight_conversation_renames.remove(&conversation_id) else {
            log::warn!(
                "complete_conversation_rename called for conversation {conversation_id:?} with no in-flight rename"
            );
            return;
        };

        if rename.attempted_title != title {
            self.apply_conversation_title(conversation_id, title, ctx);
        }
    }

    /// Reverts an in-flight rename to the captured previous title snapshot.
    pub(crate) fn fail_conversation_rename(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(rename) = self.in_flight_conversation_renames.remove(&conversation_id) else {
            log::warn!(
                "fail_conversation_rename called for conversation {conversation_id:?} with no in-flight rename"
            );
            return;
        };

        let terminal_view_id = self.terminal_view_id_for_conversation(&conversation_id);

        let mut updated = false;
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            conversation
                .restore_conversation_title(rename.previous_root_task_description, ctx);
            updated = true;
        } else {
            log::warn!(
                "fail_conversation_rename called for missing conversation {conversation_id:?}"
            );
        }

        let title = if let Some(previous_title) = rename.previous_cached_metadata_title {
            if let Some(metadata) = self.all_conversations_metadata.get_mut(&conversation_id) {
                metadata.title = previous_title.clone();
                updated = true;
            }
            previous_title
        } else {
            self.conversations_by_id
                .get(&conversation_id)
                .and_then(AIConversation::title)
                .unwrap_or_default()
        };

        if !updated {
            return;
        }

        ctx.emit(BlocklistAIHistoryEvent::UpdatedConversationTitle {
            terminal_view_id,
            conversation_id,
            title,
        });
    }

    /// Applies a conversation title locally and notifies title observers.
    pub(crate) fn apply_conversation_title(
        &mut self,
        conversation_id: AIConversationId,
        title: String,
        ctx: &mut ModelContext<Self>,
    ) {
        let terminal_view_id = self.terminal_view_id_for_conversation(&conversation_id);

        let mut updated = false;
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            conversation.update_conversation_title(title.clone(), ctx);
            updated = true;
        } else {
            log::warn!(
                "apply_conversation_title called for missing conversation {conversation_id:?}"
            );
        }

        if let Some(metadata) = self.all_conversations_metadata.get_mut(&conversation_id) {
            metadata.title = title.clone();
            updated = true;
        }

        if !updated {
            return;
        }

        ctx.emit(BlocklistAIHistoryEvent::UpdatedConversationTitle {
            terminal_view_id,
            conversation_id,
            title,
        });
    }

    /// Sets a live conversation's server token, updates the reverse index, and
    /// synchronizes any cached metadata entry for the same conversation.
    ///
    /// Returns whether the token changed. Purely in-memory: nothing here reaches
    /// SQLite, so a binding made through this method is lost on restart. Callers
    /// that need the binding to survive a restart (and live consumers to refresh)
    /// must use [`Self::set_server_conversation_token_for_conversation_and_persist`].
    pub fn set_server_conversation_token_for_conversation(
        &mut self,
        conversation_id: AIConversationId,
        token: String,
    ) -> bool {
        let new_token = ServerConversationToken::new(token.clone());
        {
            let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) else {
                return false;
            };

            let old_token = conversation.server_conversation_token().cloned();
            if old_token.as_ref() == Some(&new_token) {
                return false;
            }

            // Drop the old entry only if it still points at the given conversation_id,
            // so we don't wrongly remove an entry that's been remapped.
            if let Some(old_token) = old_token {
                if let Entry::Occupied(entry) =
                    self.server_token_to_conversation_id.entry(old_token)
                {
                    if *entry.get() == conversation_id {
                        entry.remove();
                    }
                }
            }

            conversation.set_server_conversation_token(token);
        }

        self.server_token_to_conversation_id
            .insert(new_token, conversation_id);
        self.update_cached_metadata_for_conversation(conversation_id);
        true
    }

    /// Sets a live conversation's server token, updates the reverse index,
    /// synchronizes cached metadata, persists the rebound token to SQLite, and
    /// emits refresh events for live consumers.
    ///
    /// The persisting half of the pair: [`Self::set_server_conversation_token_for_conversation`]
    /// only touches in-memory state, so a conversation bound through it alone
    /// comes back from a restart with no token and no run-id link. The handoff
    /// token is a local index in this fork (`find_conversation_id_by_server_token`,
    /// `forked_from_server_conversation_token`), so losing it loses the
    /// fork-to-source correspondence, not a cloud record.
    ///
    /// Ported from the pin (`42effe840:app/src/ai/blocklist/history_model.rs:901`).
    /// The pin's `ConversationServerTokenAssigned` event is named
    /// [`BlocklistAIHistoryEvent::ConversationAgentIdAssigned`] here, and carries a
    /// non-optional `terminal_view_id`, so it is only emitted for a conversation
    /// that is live in some terminal view -- as in the pin.
    pub fn set_server_conversation_token_for_conversation_and_persist(
        &mut self,
        conversation_id: AIConversationId,
        token: String,
        ctx: &mut ModelContext<Self>,
    ) {
        if !self.set_server_conversation_token_for_conversation(conversation_id, token) {
            return;
        }
        self.persist_conversation_state(conversation_id, ctx);
        let terminal_view_id = self.terminal_view_id_for_conversation(&conversation_id);
        ctx.emit(BlocklistAIHistoryEvent::UpdatedConversationMetadata {
            terminal_view_id,
            conversation_id,
        });
        if let Some(terminal_view_id) = terminal_view_id {
            ctx.emit(BlocklistAIHistoryEvent::ConversationAgentIdAssigned {
                conversation_id,
                terminal_view_id,
            });
        }
    }

    /// Returns the ID of the conversation that processed or is processing the response stream.
    ///
    /// A given response stream may only correspond to a single conversation at any given time,
    /// though the conversation to which it corresponds may change if a new conversation is started
    /// in the middle of the response, as is the case when the new conversation suggestion is
    /// accepted.
    pub fn conversation_for_response_stream(
        &self,
        response_stream_id: &ResponseStreamId,
    ) -> Option<AIConversationId> {
        self.conversations_by_id
            .iter()
            .find_map(|(conversation_id, conversation)| {
                if conversation.is_processing_response_stream(response_stream_id) {
                    Some(*conversation_id)
                } else {
                    None
                }
            })
    }

    pub fn conversation_status(
        &self,
        conversation_id: &AIConversationId,
    ) -> Option<&ConversationStatus> {
        self.conversation(conversation_id)
            .map(|conversation| conversation.status())
    }

    /// Returns the terminal view ID that owns the ambient-agent-run conversation with the given
    /// task id, if any.
    ///
    /// For #316: substitutes for the first branch of the pin's
    /// `ActiveAgentViewsModel::get_terminal_view_id_for_entry`
    /// (`active_agent_views_model.rs:508-524`, `02b53fcd8`) -- that model is permanently removed
    /// here (see `DECLINED.md`). Deliberately a linear scan rather than a maintained
    /// `HashMap<AmbientAgentTaskId, EntityId>` kept in sync across every conversation
    /// add/remove/clear site: reusing the same already-authoritative
    /// `live_conversation_ids_for_terminal_view` + `all_conversations_metadata` state that
    /// `terminal_view_id_for_conversation` below already scans keeps there being exactly one
    /// source of truth, with no new state that could desync from it. Same cost class
    /// (O(panes x conversations)) that method already accepts, called once per ambient-run list
    /// row.
    pub fn terminal_view_id_for_ambient_task(&self, task_id: AmbientAgentTaskId) -> Option<EntityId> {
        self.live_conversation_ids_for_terminal_view
            .iter()
            .find(|(_, conversation_ids)| {
                conversation_ids.iter().any(|conversation_id| {
                    self.all_conversations_metadata
                        .get(conversation_id)
                        .is_some_and(|metadata| metadata.ambient_agent_task_id == Some(task_id))
                })
            })
            .map(|(terminal_view_id, _)| *terminal_view_id)
    }

    /// Returns the terminal view ID that owns the given conversation, if any.
    pub fn terminal_view_id_for_conversation(
        &self,
        conversation_id: &AIConversationId,
    ) -> Option<EntityId> {
        self.live_conversation_ids_for_terminal_view
            .iter()
            .find(|(_, conversation_ids)| conversation_ids.contains(conversation_id))
            .map(|(terminal_view_id, _)| *terminal_view_id)
    }

    /// Returns the conversation ID from the terminal view's history corresponding to the action,
    /// if any.
    pub fn conversation_id_for_action(
        &self,
        action_id: &AIAgentActionId,
        terminal_view_id: EntityId,
    ) -> Option<AIConversationId> {
        self.live_conversation_ids_for_terminal_view
            .get(&terminal_view_id)?
            .iter()
            .rev()
            .find(|conversation_id| {
                self.conversations_by_id
                    .get(conversation_id)
                    .is_some_and(|conversation| conversation.contains_action(action_id))
            })
            .copied()
    }

    pub fn existing_suggestions_for_conversation(
        &self,
        conversation_id: AIConversationId,
    ) -> Option<&Suggestions> {
        self.conversations_by_id
            .get(&conversation_id)
            .and_then(|c| c.existing_suggestions())
    }

    /// The active conversation is the one we're currently or have most recently streamed outputs for.
    /// If you want to get the conversation the next query will follow up in / what is selected in the input selector,
    /// use `context_model.selected_conversation` instead.
    pub fn active_conversation(&self, terminal_view_id: EntityId) -> Option<&AIConversation> {
        self.active_conversation_id(terminal_view_id)
            .and_then(|id| self.conversation(&id))
    }

    /// True if this conversation was started from a passive entrypoint, AND the user has made no follow ups.
    pub fn is_entirely_passive_conversation(&self, conversation_id: &AIConversationId) -> bool {
        self.conversation(conversation_id)
            .is_some_and(|conversation| conversation.is_entirely_passive())
    }

    pub fn is_exchange_hidden(
        &self,
        conversation_id: AIConversationId,
        exchange_id: AIAgentExchangeId,
    ) -> bool {
        self.conversations_by_id
            .get(&conversation_id)
            .is_some_and(|c| c.is_exchange_hidden(exchange_id))
    }

    /// Add a new [`AIAgentExchange`] to the [`AIConversation`] with the given [`AIConversationId`].
    /// Emits an event with the new exchange.
    pub(super) fn update_conversation_for_new_request_input(
        &mut self,
        request_input: RequestInput,
        stream_id: ResponseStreamId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Result<(), UpdateHistoryError> {
        let conversation = self
            .conversations_by_id
            .get_mut(&request_input.conversation_id)
            .ok_or(UpdateHistoryError::ConversationNotFound(
                request_input.conversation_id,
            ))?;
        conversation.update_for_new_request_input(
            request_input,
            stream_id,
            terminal_view_id,
            ctx,
        )?;
        Ok(())
    }

    pub fn append_byop_preflight_messages_to_task(
        &mut self,
        conversation_id: AIConversationId,
        task_id: TaskId,
        messages: Vec<warp_multi_agent_api::Message>,
        ctx: &mut ModelContext<Self>,
    ) -> Result<usize, UpdateHistoryError> {
        let conversation = self
            .conversations_by_id
            .get_mut(&conversation_id)
            .ok_or(UpdateHistoryError::ConversationNotFound(conversation_id))?;
        conversation
            .append_byop_preflight_messages_to_task(task_id, messages, ctx)
            .map_err(UpdateHistoryError::from)
    }

    pub fn restore_conversations(
        &mut self,
        terminal_view_id: EntityId,
        conversations: Vec<AIConversation>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.terminal_view_created_at
            .insert(terminal_view_id, Local::now());

        let mut conversation_ids = Vec::new();
        for conversation in conversations.into_iter() {
            let conversation_id = conversation.id();
            conversation_ids.push(conversation_id);
            self.live_conversation_ids_for_terminal_view
                .entry(terminal_view_id)
                .or_default()
                .push(conversation_id);

            if let Some(key) = agent_id_key(&conversation) {
                self.agent_id_to_conversation_id
                    .insert(key, conversation_id);
            }

            if let Some(token) = conversation.server_conversation_token() {
                self.server_token_to_conversation_id
                    .insert(token.clone(), conversation_id);
            }

            // Maintain the parent→child index for child agent conversations.
            // Resolve through `resolved_parent_conversation_id_for_conversation`
            // rather than reading `parent_conversation_id()` directly, so a child
            // that carries only a `parent_agent_id` (the link a restored remote
            // child usually has) still lands under its parent. The `agent_id_key`
            // seeding above means a parent restored earlier in this same loop —
            // or in an earlier `restore_conversations` call — is already in the
            // `agent_id_to_conversation_id` index by the time we get here. Matches
            // the pin (`42effe840:app/src/ai/blocklist/history_model.rs:1145-1149`).
            let resolved_parent_id =
                self.resolved_parent_conversation_id_for_conversation(&conversation);
            if let Some(parent_id) = resolved_parent_id {
                let children = self.children_by_parent.entry(parent_id).or_default();
                if !children.contains(&conversation_id) {
                    children.push(conversation_id);
                }
            }

            let new_status = conversation.status().clone();
            self.conversations_by_id
                .insert(conversation_id, conversation);

            // Emit UpdatedConversationStatus for restored conversations so that
            // the workspace can set tab indicators appropriately
            ctx.emit(BlocklistAIHistoryEvent::UpdatedConversationStatus {
                conversation_id,
                terminal_view_id,
                update: ConversationStatusUpdate::Restored,
                new_status,
            });
        }

        // Emit event so AI document views can populate their terminal view references
        ctx.emit(BlocklistAIHistoryEvent::RestoredConversations {
            terminal_view_id,
            conversation_ids,
        });
    }

    /// Sets the active conversation ID, transferring ownership from any other
    /// terminal view that currently holds it.
    ///
    /// Use this when the user **explicitly navigates** to a conversation in a
    /// different view (e.g. from the conversation history or command palette).
    /// For automatic follow-ups during tool-call cycles, use [`Self::mark_active_conversation_id`]
    /// instead — it updates the active pointer without touching other views.
    pub fn set_active_conversation_id(
        &mut self,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        if !self
            .live_conversation_ids_for_terminal_view
            .get(&terminal_view_id)
            .is_some_and(|conversation_ids| conversation_ids.contains(&conversation_id))
        {
            log::error!(
                "Attempted to set active conversation ID for terminal view ID that does not own that conversation."
            );
            return;
        }

        let mut previous_terminal_views = Vec::new();
        for (other_terminal_view, other_terminal_view_live_conversation_ids) in self
            .live_conversation_ids_for_terminal_view
            .iter_mut()
            .filter(|(other_terminal_view_id, _)| **other_terminal_view_id != terminal_view_id)
        {
            if let Some(pos) = other_terminal_view_live_conversation_ids
                .iter()
                .position(|id| *id == conversation_id)
            {
                other_terminal_view_live_conversation_ids.remove(pos);
                previous_terminal_views.push(*other_terminal_view);
            }

            if self
                .active_conversation_for_terminal_view
                .get(other_terminal_view)
                .is_some_and(|id| *id == conversation_id)
            {
                self.active_conversation_for_terminal_view
                    .remove(other_terminal_view);
                ctx.emit(BlocklistAIHistoryEvent::ClearedActiveConversation {
                    conversation_id,
                    terminal_view_id: *other_terminal_view,
                });
            }
        }
        for previous_terminal_view_id in previous_terminal_views {
            ctx.emit(
                BlocklistAIHistoryEvent::ConversationTransferredBetweenTerminalViews {
                    conversation_id,
                    previous_terminal_view_id,
                    new_terminal_view_id: terminal_view_id,
                },
            );
        }

        self.active_conversation_for_terminal_view
            .insert(terminal_view_id, conversation_id);

        ctx.emit(BlocklistAIHistoryEvent::SetActiveConversation {
            conversation_id,
            terminal_view_id,
        });
    }

    /// Marks a conversation as the active conversation for a terminal view
    /// **without** removing it from other views.
    ///
    /// This is the non-transferring counterpart to [`Self::set_active_conversation_id`].
    /// Use this during automatic follow-ups and request sending where the
    /// conversation already belongs to this view and we only need to update
    /// the "most recently streamed" pointer.
    pub fn mark_active_conversation_id(
        &mut self,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        if !self
            .live_conversation_ids_for_terminal_view
            .get(&terminal_view_id)
            .is_some_and(|conversation_ids| conversation_ids.contains(&conversation_id))
        {
            log::warn!(
                "mark_active_conversation_id: conversation {conversation_id:?} is not in \
                 terminal view {terminal_view_id:?} live list, skipping"
            );
            return;
        }

        self.active_conversation_for_terminal_view
            .insert(terminal_view_id, conversation_id);

        ctx.emit(BlocklistAIHistoryEvent::SetActiveConversation {
            conversation_id,
            terminal_view_id,
        });
    }

    /// Starts a new conversation in the given terminal view's history, effectively marking the
    /// existing conversation (if any) as completed.
    ///
    /// Returns the ID of the created conversation.
    ///
    /// Conversation completion is inferred if the conversation in question is _not_ the last
    /// element in the `conversations` vector.
    pub fn start_new_conversation(
        &mut self,
        terminal_view_id: EntityId,
        is_autoexecute_override: bool,
        is_viewing_shared_session: bool,
        ctx: &mut ModelContext<Self>,
    ) -> AIConversationId {
        let mut new_conversation = AIConversation::new(is_viewing_shared_session);
        if is_autoexecute_override {
            new_conversation.toggle_autoexecute_override();
        }
        let new_conversation_id = new_conversation.id();
        self.live_conversation_ids_for_terminal_view
            .entry(terminal_view_id)
            .or_default()
            .push(new_conversation_id);
        self.conversations_by_id
            .insert(new_conversation_id, new_conversation);

        ctx.emit(BlocklistAIHistoryEvent::StartedNewConversation {
            new_conversation_id,
            terminal_view_id,
        });

        new_conversation_id
    }

    pub fn create_cli_subagent_task_for_conversation(
        &mut self,
        block_id: BlockId,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Result<TaskId, UpdateHistoryError> {
        let conversation = self
            .conversations_by_id
            .get_mut(&conversation_id)
            .ok_or(UpdateHistoryError::ConversationNotFound(conversation_id))?;
        Ok(conversation.create_optimistic_cli_subagent_task(&block_id, terminal_view_id, ctx))
    }

    /// Zap BYOP-only: see the `Conversation::create_optimistic_cli_subagent_task_silent`
    /// docs. Creates a real subagent task but doesn't emit `CreatedSubtask`, avoiding
    /// triggering a `CLISubagentView` panic when the task has no exchange yet.
    pub fn create_silent_cli_subagent_task_for_conversation(
        &mut self,
        block_id: BlockId,
        conversation_id: AIConversationId,
    ) -> Result<TaskId, UpdateHistoryError> {
        let conversation = self
            .conversations_by_id
            .get_mut(&conversation_id)
            .ok_or(UpdateHistoryError::ConversationNotFound(conversation_id))?;
        Ok(conversation.create_optimistic_cli_subagent_task_silent(&block_id))
    }

    pub fn update_conversation_status(
        &mut self,
        terminal_view_id: EntityId,
        conversation_id: AIConversationId,
        status: ConversationStatus,
        ctx: &mut ModelContext<Self>,
    ) {
        self.update_conversation_status_with_error_message(
            terminal_view_id,
            conversation_id,
            status,
            None,
            ctx,
        );
    }

    pub fn update_conversation_status_with_error_message(
        &mut self,
        terminal_view_id: EntityId,
        conversation_id: AIConversationId,
        status: ConversationStatus,
        error_message: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) {
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            conversation.update_status_with_error_message(
                status,
                error_message,
                terminal_view_id,
                ctx,
            );
        }
    }

    pub fn on_forked_conversation(
        &mut self,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        // When a conversation is forked and restored into a new terminal view,
        // we want to emit UpdatedStreamingExchange events for every exchange
        // to ensure that all of the existing exchanges are persisted correctly.
        if let Some(conversation) = self.conversations_by_id.get(&conversation_id) {
            for exchange in conversation.all_exchanges().into_iter() {
                let is_hidden = conversation.is_exchange_hidden(exchange.id);
                ctx.emit(BlocklistAIHistoryEvent::UpdatedStreamingExchange {
                    exchange_id: exchange.id,
                    terminal_view_id,
                    conversation_id,
                    is_hidden,
                });
            }
        }
    }

    pub fn initialize_output_for_response_stream(
        &mut self,
        stream_id: &ResponseStreamId,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        init_event: warp_multi_agent_api::response_event::StreamInit,
        ctx: &mut ModelContext<Self>,
    ) {
        // Hoisted out of the `&mut` borrow of `conversations_by_id` below:
        // `persist_conversation_state` takes `&mut self`, so it cannot run while
        // `conversation` is still live. Matches the pin
        // (`42effe840:app/src/ai/blocklist/history_model.rs:1388-1418`).
        let mut should_emit_agent_id_assigned = false;
        let mut should_persist = false;
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            let had_token_before = conversation.server_conversation_token().is_some();

            if let Err(e) = conversation.initialize_output_for_response_stream(
                stream_id,
                init_event,
                terminal_view_id,
                ctx,
            ) {
                log::warn!("Failed to update conversation with updated streamed output: {e}");
            }

            if let Some(key) = agent_id_key(conversation) {
                self.agent_id_to_conversation_id
                    .insert(key, conversation_id);
            }

            if let Some(token) = conversation.server_conversation_token() {
                self.server_token_to_conversation_id
                    .insert(token.clone(), conversation_id);
            }

            // Notify observers when a conversation first receives its agent id.
            should_emit_agent_id_assigned =
                !had_token_before && conversation.server_conversation_token().is_some();
            should_persist = true;
        }

        // The server conversation token and run id assigned by `StreamInit` are
        // durable conversation fields: without this write they live only in
        // memory and the conversation comes back from a restart with no server
        // identity at all.
        if should_persist {
            self.persist_conversation_state(conversation_id, ctx);
        }

        if should_emit_agent_id_assigned {
            ctx.emit(BlocklistAIHistoryEvent::ConversationAgentIdAssigned {
                conversation_id,
                terminal_view_id,
            });
        }
    }

    /// Assigns a `run_id` to a conversation that was spawned as a remote child
    /// agent. Updates the `agent_id_to_conversation_id` index and emits
    /// `ConversationAgentIdAssigned` so restored child-agent links can resolve.
    pub fn assign_run_id_for_conversation(
        &mut self,
        conversation_id: AIConversationId,
        run_id: String,
        task_id: Option<crate::ai::ambient_agents::AmbientAgentTaskId>,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        // Scoped so the `&mut` borrow of `conversations_by_id` ends before
        // `persist_conversation_state` (which takes `&mut self`) is called.
        let (agent_key, server_token) = {
            let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) else {
                log::warn!(
                    "assign_run_id_for_conversation: conversation {conversation_id:?} not found"
                );
                return;
            };
            conversation.set_run_id(run_id);
            if let Some(task_id) = task_id {
                conversation.set_task_id(task_id);
            }
            (
                agent_id_key(conversation),
                conversation.server_conversation_token().cloned(),
            )
        };

        if let Some(key) = agent_key {
            self.agent_id_to_conversation_id
                .insert(key, conversation_id);
        }

        if let Some(token) = server_token {
            self.server_token_to_conversation_id
                .insert(token, conversation_id);
        }

        // The run id is a durable conversation field: without this write it lives
        // only in memory and the conversation comes back from a restart with no
        // run, breaking the child-agent links that `conversation_id_for_agent_id`
        // resolves. The pin persists here too
        // (`42effe840:app/src/ai/blocklist/history_model.rs:1466`).
        self.persist_conversation_state(conversation_id, ctx);
        ctx.emit(BlocklistAIHistoryEvent::ConversationAgentIdAssigned {
            conversation_id,
            terminal_view_id,
        });
    }

    /// Resolves a server-side agent identifier to a local conversation ID.
    /// The identifier may be a server conversation token (v1) or a run_id (v2).
    ///
    /// Falls back to `server_token_to_conversation_id` when there's no
    /// `agent_id_to_conversation_id` entry: that index is only populated via
    /// `assign_run_id_for_conversation` (the run_id/v2 path), so a
    /// conversation identified purely by a v1 server_conversation_token --
    /// e.g. `set_server_conversation_token_for_conversation` without a
    /// run_id -- would otherwise never resolve here even though
    /// `agent_link_id()` (used to key `agent_id_to_conversation_id` in the
    /// first place) treats the two as equivalent, falling back to the token
    /// itself when there's no run_id.
    pub fn conversation_id_for_agent_id(&self, agent_id: &str) -> Option<AIConversationId> {
        self.agent_id_to_conversation_id
            .get(agent_id)
            .copied()
            .or_else(|| {
                self.server_token_to_conversation_id
                    .get(&ServerConversationToken::new(agent_id.to_owned()))
                    .copied()
            })
    }

    /// Creates a new conversation and transfers relevant exchanges from
    /// the existing conversation to the new one. If successful, returns the new conversation id.
    fn handle_conversation_split(
        &mut self,
        old_conversation_id: AIConversationId,
        response_stream_id: &ResponseStreamId,
        start_from_message_id: MessageId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Result<AIConversationId, UpdateHistoryError> {
        let exchange_ids_to_transfer: Vec<AIAgentExchangeId> = self
            .conversation(&old_conversation_id)
            .ok_or(UpdateHistoryError::ConversationNotFound(
                old_conversation_id,
            ))?
            .all_exchanges()
            .into_iter()
            .skip_while(|e| !e.added_message_ids.contains(&start_from_message_id))
            .map(|e| e.id)
            .collect();

        if exchange_ids_to_transfer.is_empty() {
            log::warn!("Starting a new conversation: message id not found");
            return Err(UpdateHistoryError::Conversation(
                UpdateConversationError::ExchangeNotFound,
            ));
        }
        log::info!(
            "Starting a new conversation: transferring {} exchanges to new conversation",
            exchange_ids_to_transfer.len()
        );

        let new_conversation_id = self.start_new_conversation(terminal_view_id, false, false, ctx);
        for exchange_id in exchange_ids_to_transfer {
            let old_conversation = self
                .conversations_by_id
                .get_mut(&old_conversation_id)
                .ok_or(UpdateHistoryError::ConversationNotFound(
                    old_conversation_id,
                ))?;
            let exchange = old_conversation.remove_exchange(exchange_id)?;

            let new_conversation = self
                .conversations_by_id
                .get_mut(&new_conversation_id)
                .ok_or(UpdateHistoryError::ConversationNotFound(
                    new_conversation_id,
                ))?;
            new_conversation.append_reassigned_exchange(
                response_stream_id,
                exchange,
                terminal_view_id,
                ctx,
            )?;
        }

        // Mark the old conversation as complete since we're starting a new one
        let old_conversation = self
            .conversations_by_id
            .get_mut(&old_conversation_id)
            .ok_or(UpdateHistoryError::ConversationNotFound(
                old_conversation_id,
            ))?;
        old_conversation.mark_completed_after_successful_split(
            response_stream_id,
            terminal_view_id,
            ctx,
        )?;

        self.set_active_conversation_id(new_conversation_id, terminal_view_id, ctx);

        ctx.emit(BlocklistAIHistoryEvent::SplitConversation {
            terminal_view_id,
            old_conversation_id,
            new_conversation_id,
        });

        Ok(new_conversation_id)
    }

    /// Checks whether a conversation can be forked without mutating history.
    pub fn validate_fork_source(
        source_conversation: &AIConversation,
    ) -> Result<(), ForkConversationError> {
        if source_conversation.is_empty() {
            return Err(ForkConversationError::EmptyConversation);
        }
        Ok(())
    }

    /// Forks an existing conversation by creating a new conversation
    /// and copying the existing conversation's tasks into the new conversation.
    ///
    /// The `prefix` parameter specifies the prefix added to the root task description
    /// (e.g., `FORK_PREFIX` for forks, `PRE_REWIND_PREFIX` for pre-rewind backups),
    /// unless `title_override` is set, in which case it replaces the prefixed
    /// description entirely.
    ///
    /// `preserve_task_ids` keeps the root and subtask ids (and the subtask's
    /// `parent_task_id` back-reference) stable across the fork instead of
    /// regenerating every id. The ordinary user-facing fork passes `true` so
    /// the forked conversation's tasks keep referring to the same ids the
    /// user has already seen; the pre-rewind backup passes `false` because
    /// that backup coexists with the live conversation under the same task
    /// ids otherwise being reused for new writes.
    pub fn fork_conversation(
        &mut self,
        source_conversation: &AIConversation,
        prefix: &str,
        preserve_task_ids: bool,
        title_override: Option<&str>,
        app: &AppContext,
    ) -> Result<AIConversation, anyhow::Error> {
        Self::validate_fork_source(source_conversation)?;
        let tasks: Vec<warp_multi_agent_api::Task> = source_conversation
            .all_tasks()
            .filter_map(|t| t.source().cloned())
            .collect();

        let updated_tasks_with_new_ids =
            update_forked_task_properties(tasks.clone(), prefix, preserve_task_ids, title_override);
        let byop_repair_state_json = byop_fork_repair_state_json(
            &tasks,
            &updated_tasks_with_new_ids,
            &source_conversation.byop_repair_state,
            None,
        );
        let Some(sqlite_sender) = GlobalResourceHandlesProvider::as_ref(app)
            .get()
            .model_event_sender
            .clone()
        else {
            return Err(anyhow!("No sqlite sender available."));
        };

        // We preserve reverted action IDs. Orphaned IDs (for actions not in fork) are harmless.
        // The reverted states are only copied to the new conversation if the revert happened before the user clicked fork,
        // but regardless of when the revert happened relative to the fork point.
        //
        // Example:
        // 1. Agent edit action
        // 2. Agent edit action
        // 3. User reverts edit from 1
        // 4. **User clicks fork**
        // 5. User reverts edit from 2
        //
        // In this example, the forked conversation will always show edit 1 as reverted and edit 2 as not reverted,
        // regardless of if the fork point is between 2 and 3 or 3 and 4. This is because we preserve all prior reverts,
        // either if they game before or after the fork point. However, once forked, we don't copy later reverts.
        let reverted_action_ids = if source_conversation.reverted_action_ids().is_empty() {
            None
        } else {
            Some(
                source_conversation
                    .reverted_action_ids()
                    .clone()
                    .into_iter()
                    .map_into()
                    .collect(),
            )
        };

        let conversation_data = AgentConversationData {
            is_remote_child: false,
            server_conversation_token: None,
            conversation_usage_metadata: Some(source_conversation.usage_metadata()),
            reverted_action_ids,
            forked_from_server_conversation_token: source_conversation
                .server_conversation_token()
                .map(|t| t.as_str().to_string()),
            // We reset artifacts on fork
            artifacts_json: None,
            // Forked conversation loses its parentage
            parent_agent_id: None,
            agent_name: None,
            orchestration_harness_type: None,
            root_task_is_optimistic: None,
            parent_conversation_id: None,
            run_id: None,
            autoexecute_override: Some(source_conversation.autoexecute_override().into()),
            // The event cursor belongs to the source conversation's run; the
            // forked conversation will establish its own cursor.
            last_event_sequence: None,
            // A forked conversation is a fresh top-level conversation, not a
            // pinned orchestration child.
            pinned: false,
            compaction_state_json: None,
            byop_repair_state_json,
            cli_subagent_block_snapshots_json: None,
        };
        let forked_conversation_id = AIConversationId::new();
        if let Err(e) = sqlite_sender.send(ModelEvent::UpdateMultiAgentConversation {
            conversation_id: forked_conversation_id.to_string(),
            updated_tasks: updated_tasks_with_new_ids.clone(),
            conversation_data: conversation_data.clone(),
        }) {
            return Err(anyhow!("Failed to persist forked conversation: {e:?}."));
        }

        // Insert this conversation into the history model memory so we don't need to read from DB to restore this forked conversation
        // (otherwise, we can run into a race condition where the conversation is not found in the DB because we haven't finished writing to the db).
        let forked_conversation = self.insert_forked_conversation_from_tasks(
            forked_conversation_id,
            updated_tasks_with_new_ids.clone(),
            conversation_data.clone(),
        )?;

        Ok(forked_conversation)
    }

    /// Forks an existing conversation at a specific exchange boundary. When `exact_exchange`
    /// is true, the fork includes all messages up to and including the selected exchange.
    /// Otherwise, it extends through the full response (every message after the user's query
    /// until the next root-task user query).
    ///
    /// The `prefix` parameter specifies the prefix added to the root task description
    /// (e.g., `FORK_PREFIX` for forks, `PRE_REWIND_PREFIX` for pre-rewind backups).
    pub fn fork_conversation_at_exchange(
        &mut self,
        source_conversation: &AIConversation,
        from_exchange_id: AIAgentExchangeId,
        fork_from_exact_exchange: bool,
        prefix: &str,
        app: &AppContext,
    ) -> Result<AIConversation, anyhow::Error> {
        let conversation = source_conversation;

        let exchanges_by_task: Vec<(TaskId, Vec<&AIAgentExchange>)> =
            conversation.all_exchanges_by_task();

        let root_task_id = conversation.get_root_task_id().clone();

        let mut message_ids_to_retain_by_task: HashMap<TaskId, HashSet<MessageId>> = HashMap::new();
        let mut found_from_exchange_id = false;
        'outer: for (task_id, task_exchanges) in exchanges_by_task.into_iter() {
            for exchange in task_exchanges {
                // In the non-exact case, we continue past the selected exchange until we reach
                // the next user query (effectively forking from the selected 'response').
                if found_from_exchange_id && task_id == root_task_id && exchange.has_user_query() {
                    break 'outer;
                }

                let message_ids_to_retain = message_ids_to_retain_by_task
                    .entry(task_id.clone())
                    .or_default();
                message_ids_to_retain.extend(exchange.added_message_ids.iter().cloned());
                if exchange.id == from_exchange_id {
                    if fork_from_exact_exchange {
                        break 'outer;
                    }
                    found_from_exchange_id = true;
                }
            }
        }

        if message_ids_to_retain_by_task.is_empty() {
            return Err(anyhow!(
                "No messages found for block in conversation {}.",
                conversation.id()
            ));
        }

        // Build truncated tasks by retaining only messages whose IDs are in
        // `allowed_message_ids`. Tasks whose message list becomes empty and
        // which are non-root tasks are dropped.
        let source_tasks: Vec<warp_multi_agent_api::Task> = conversation
            .all_tasks()
            .filter_map(|t| t.source().cloned())
            .collect();
        let truncated_tasks: Vec<warp_multi_agent_api::Task> = source_tasks
            .iter()
            .filter_map(|t| {
                if let Some(message_ids_to_retain) =
                    message_ids_to_retain_by_task.get(&TaskId::new(t.id.clone()))
                {
                    let mut truncated_task = t.clone();
                    truncated_task
                        .messages
                        .retain(|m| message_ids_to_retain.contains(&MessageId::new(m.id.clone())));
                    (!truncated_task.messages.is_empty()).then_some(truncated_task)
                } else {
                    None
                }
            })
            .collect();

        if truncated_tasks.is_empty() {
            return Err(anyhow!(
                "Truncated tasks for forked conversation at block are empty for conversation {}.",
                conversation.id()
            ));
        }

        // An exact fork cuts at the end of the fork-point exchange, which can land
        // between a tool_call and its result. Every unpaired client tool_call left
        // at that boundary has to be reconciled, or the forked conversation opens
        // with a `tool_use` no provider will accept.
        let mut truncated_tasks = truncated_tasks;
        if let Some(fork_point_exchange) = conversation.exchange_with_id(from_exchange_id) {
            let fork_point_message_ids: HashSet<MessageId> = fork_point_exchange
                .added_message_ids
                .iter()
                .cloned()
                .collect();
            reconcile_fork_point_tool_calls(
                &mut truncated_tasks,
                &source_tasks,
                &root_task_id,
                &fork_point_message_ids,
            );
        }

        let updated_tasks_with_new_ids = update_forked_task_properties(
            truncated_tasks,
            prefix,
            true, /* preserve_task_ids */
            None,
        );
        let byop_repair_state_json = byop_fork_repair_state_json(
            &source_tasks,
            &updated_tasks_with_new_ids,
            &conversation.byop_repair_state,
            Some(from_exchange_id.to_string()),
        );

        let Some(sqlite_sender) = GlobalResourceHandlesProvider::as_ref(app)
            .get()
            .model_event_sender
            .clone()
        else {
            return Err(anyhow!("No sqlite sender available."));
        };

        // We preserve reverted action IDs. Orphaned IDs (for actions not in fork) are harmless.
        // The reverted states are only copied to the new conversation if the revert happened before the user clicked fork,
        // but regardless of when the revert happened relative to the fork point.
        //
        // Example:
        // 1. Agent edit action
        // 2. Agent edit action
        // 3. User reverts edit from 1
        // 4. **User clicks fork**
        // 5. User reverts edit from 2
        //
        // In this example, the forked conversation will always show edit 1 as reverted and edit 2 as not reverted,
        // regardless of if the fork point is between 2 and 3 or 3 and 4. This is because we preserve all prior reverts,
        // either if they game before or after the fork point. However, once forked, we don't copy later reverts.
        let reverted_action_ids = if conversation.reverted_action_ids().is_empty() {
            None
        } else {
            Some(
                conversation
                    .reverted_action_ids()
                    .clone()
                    .into_iter()
                    .map_into()
                    .collect(),
            )
        };

        // Start forked conversations without usage metadata for now; this can
        // be recomputed based on the retained exchanges in a follow-up.
        let conversation_data = AgentConversationData {
            is_remote_child: false,
            server_conversation_token: None,
            conversation_usage_metadata: None,
            reverted_action_ids,
            forked_from_server_conversation_token: conversation
                .server_conversation_token()
                .map(|t| t.as_str().to_string()),
            // We reset artifacts on fork
            artifacts_json: None,
            // Forked conversation loses its parentage.
            parent_agent_id: None,
            agent_name: None,
            orchestration_harness_type: None,
            root_task_is_optimistic: None,
            parent_conversation_id: None,
            run_id: None,
            autoexecute_override: Some(conversation.autoexecute_override().into()),
            // The event cursor belongs to the source conversation's run; the
            // forked conversation will establish its own cursor.
            last_event_sequence: None,
            // A forked conversation is a fresh top-level conversation, not a
            // pinned orchestration child.
            pinned: false,
            compaction_state_json: None,
            byop_repair_state_json,
            cli_subagent_block_snapshots_json: None,
        };

        let forked_conversation_id = AIConversationId::new();
        if let Err(e) = sqlite_sender.send(ModelEvent::UpdateMultiAgentConversation {
            conversation_id: forked_conversation_id.to_string(),
            updated_tasks: updated_tasks_with_new_ids.clone(),
            conversation_data: conversation_data.clone(),
        }) {
            return Err(anyhow!(
                "Failed to persist forked conversation at block: {e:?}."
            ));
        }

        let forked_conversation = self.insert_forked_conversation_from_tasks(
            forked_conversation_id,
            updated_tasks_with_new_ids,
            conversation_data,
        )?;

        Ok(forked_conversation)
    }

    pub fn apply_client_actions(
        &mut self,
        response_stream_id: &ResponseStreamId,
        client_actions: Vec<warp_multi_agent_api::ClientAction>,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        skill_path_origin: &SkillPathOrigin,
        ctx: &mut ModelContext<Self>,
    ) -> Result<(), UpdateHistoryError> {
        let mut current_conversation_id = conversation_id;
        for client_action in client_actions {
            match client_action.action {
                Some(Action::StartNewConversation(StartNewConversation {
                    start_from_message_id,
                })) => {
                    let new_conversation_id = self.handle_conversation_split(
                        current_conversation_id,
                        response_stream_id,
                        MessageId::new(start_from_message_id),
                        terminal_view_id,
                        ctx,
                    )?;
                    current_conversation_id = new_conversation_id;
                }
                Some(action) => {
                    let conversation = self
                        .conversations_by_id
                        .get_mut(&current_conversation_id)
                        .ok_or(UpdateHistoryError::ConversationNotFound(
                        current_conversation_id,
                    ))?;
                    conversation.apply_client_action(
                        response_stream_id,
                        terminal_view_id,
                        action,
                        skill_path_origin,
                        ctx,
                    )?;
                }
                None => {
                    log::warn!("Received empty client action");
                }
            }
        }
        Ok(())
    }

    pub fn update_conversation_cost_and_usage_for_request(
        &mut self,
        conversation_id: AIConversationId,
        request_cost: Option<RequestCost>,
        token_usage: Vec<TokenUsage>,
        usage_metadata: Option<ConversationUsageMetadata>,
        was_user_initiated_request: bool,
    ) {
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            if let Err(e) = conversation.update_cost_and_usage_for_request(
                request_cost,
                token_usage,
                usage_metadata,
                was_user_initiated_request,
            ) {
                log::warn!(
                    "Failed to update request cost for conversation {conversation_id}: {e:#}"
                );
            }
        } else {
            log::warn!(
                "Failed to update request cost because conversation {conversation_id} was not found"
            );
        }
    }

    pub fn mark_response_stream_completed_successfully(
        &mut self,
        stream_id: &ResponseStreamId,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) else {
            return;
        };
        if let Err(e) = conversation.mark_request_completed(stream_id, terminal_view_id, ctx) {
            log::warn!("Failed to mark exchange as completed: {e}");
        }
    }

    pub fn set_exchange_time_to_first_token(
        &mut self,
        conversation_id: AIConversationId,
        exchange_id: AIAgentExchangeId,
        time_to_first_token_ms: i64,
    ) {
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            if let Ok(exchange) = conversation.get_exchange_to_update(exchange_id) {
                exchange.time_to_first_token_ms = Some(time_to_first_token_ms);
            }
        }
    }

    pub fn mark_response_stream_cancelled(
        &mut self,
        stream_id: &ResponseStreamId,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        reason: CancellationReason,
        ctx: &mut ModelContext<Self>,
    ) {
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            if reason.is_reverted() {
                if let Err(e) =
                    conversation.mark_request_cancelled_due_to_revert(terminal_view_id, ctx)
                {
                    log::warn!("Failed to mark exchange as cancelled: {e}");
                }
            } else { match conversation.mark_request_cancelled(stream_id, terminal_view_id, reason, ctx)
            { Err(e) => {
                log::warn!("Failed to mark exchange as cancelled: {e}");
            } _ => {}}}
        }
        AIDocumentModel::handle(ctx).update(ctx, |model, ctx| {
            model.clear_streaming_documents_for_conversation(&conversation_id, ctx);
        });
    }

    pub fn mark_response_stream_completed_with_error(
        &mut self,
        error: RenderableAIError,
        recovery_pending: bool,
        stream_id: &ResponseStreamId,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            if let Err(e) = conversation.mark_request_completed_with_error(
                stream_id,
                error.clone(),
                recovery_pending,
                terminal_view_id,
                ctx,
            ) {
                log::warn!("Failed to mark exchange as completed with error: {e}");
            }
        }
    }

    /// Handle clearing the blocklist for the terminal view.
    /// The terminal view will also cancel the active stream on processing the event emitted here.
    pub fn clear_conversations_in_terminal_view(
        &mut self,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        // Cancel the active stream when we clear conversations in this terminal view.
        let active_conversation_id = self
            .active_conversation_for_terminal_view
            .remove(&terminal_view_id);
        if let Some(cleared_conversation_ids) = self
            .live_conversation_ids_for_terminal_view
            .remove(&terminal_view_id)
        {
            self.cleared_conversation_ids_for_terminal_view
                .entry(terminal_view_id)
                .and_modify(|existing| existing.extend(cleared_conversation_ids.clone()))
                .or_insert(cleared_conversation_ids);
        }
        let cleared_conversation_ids = self
            .live_conversation_ids_for_terminal_view
            .remove(&terminal_view_id);
        if let Some(cleared_conversation_ids) = cleared_conversation_ids {
            self.cleared_conversation_ids_for_terminal_view
                .entry(terminal_view_id)
                .and_modify(|existing| existing.extend(cleared_conversation_ids.clone()))
                .or_insert(cleared_conversation_ids);
        }
        ctx.emit(
            BlocklistAIHistoryEvent::ClearedConversationsInTerminalView {
                terminal_view_id,
                active_conversation_id,
            },
        );
    }

    /// Handle removing a conversation from the history model, blocklist and in-memory.
    pub fn remove_conversation(
        &mut self,
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.remove_conversation_from_memory(conversation_id, Some(terminal_view_id), ctx);
    }

    /// Permanently delete a conversation.
    pub fn delete_conversation(
        &mut self,
        conversation_id: AIConversationId,
        terminal_view_id: Option<EntityId>,
        ctx: &mut ModelContext<Self>,
    ) {
        let conversation_title = self
            .conversations_by_id
            .get(&conversation_id)
            .and_then(|c| c.title().map(|t| t.to_string()));

        self.remove_conversation_from_memory(conversation_id, terminal_view_id, ctx);

        // Delete persisted conversation from sqlite.
        let model_event_sender = GlobalResourceHandlesProvider::as_ref(ctx)
            .get()
            .model_event_sender
            .clone();
        let conversation_id_string = conversation_id.to_string();
        ctx.spawn(
            async move {
                if let Some(sender) = model_event_sender {
                    if let Err(e) = sender.send(ModelEvent::DeleteAIConversation {
                        conversation_id: conversation_id_string.clone(),
                    }) {
                        log::error!("Error sending DeleteAIConversation event: {e:?}");
                    }
                    if let Err(e) = sender.send(ModelEvent::DeleteMultiAgentConversations {
                        conversation_ids: vec![conversation_id_string],
                    }) {
                        log::error!("Error sending DeleteMultiAgentConversations event: {e:?}");
                    }
                }
            },
            |_, _, _| {},
        );

        // Always emit the event so subscribers (e.g. AgentConversationsModel,
        // the conversation list UI) can refresh their caches. Historical-only
        // conversations have no terminal_view_id; handlers that filter on it
        // already tolerate `None`.
        ctx.emit(BlocklistAIHistoryEvent::DeletedConversation {
            terminal_view_id,
            conversation_id,
            conversation_title,
        });
    }

    /// Remove a conversation from all in-memory storage.
    fn remove_conversation_from_memory(
        &mut self,
        conversation_id: AIConversationId,
        terminal_view_id: Option<EntityId>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Clean up reverse indices before removing the conversation. Guard
        // token-index removals with an equality check: the live conversation's
        // token and the metadata's token can diverge after a rebind, and we
        // must not clobber an entry already owned by another conversation.
        if let Some(conversation) = self.conversations_by_id.get(&conversation_id) {
            if let Some(key) = agent_id_key(conversation) {
                self.agent_id_to_conversation_id.remove(&key);
            }
            if let Some(token) = conversation.server_conversation_token() {
                if self.server_token_to_conversation_id.get(token) == Some(&conversation_id) {
                    self.server_token_to_conversation_id.remove(token);
                }
            }
        }
        // Also clean up the token index entry that might have been installed
        // via the metadata path (no live conversation present).
        if let Some(metadata) = self.all_conversations_metadata.get(&conversation_id) {
            if let Some(token) = &metadata.server_conversation_token {
                if self.server_token_to_conversation_id.get(token) == Some(&conversation_id) {
                    self.server_token_to_conversation_id.remove(token);
                }
            }
        }

        self.all_conversations_metadata.remove(&conversation_id);
        self.conversations_by_id.remove(&conversation_id);

        // Clean up the parent→child index: drop this conversation's own child
        // list (outgoing edges) and remove it from any parent's list (incoming
        // edge), pruning parents that become empty.
        self.children_by_parent.remove(&conversation_id);
        self.children_by_parent.retain(|_, child_ids| {
            child_ids.retain(|child_id| *child_id != conversation_id);
            !child_ids.is_empty()
        });

        if let Some(terminal_view_id) = terminal_view_id {
            if self
                .active_conversation_for_terminal_view
                .get(&terminal_view_id)
                .is_some_and(|id| *id == conversation_id)
            {
                self.active_conversation_for_terminal_view
                    .remove(&terminal_view_id);
            }
            if let Some(vec) = self
                .live_conversation_ids_for_terminal_view
                .get_mut(&terminal_view_id)
            {
                vec.retain(|&id| id != conversation_id);
            }
            if let Some(vec) = self
                .cleared_conversation_ids_for_terminal_view
                .get_mut(&terminal_view_id)
            {
                vec.retain(|&id| id != conversation_id);
            }
            ctx.emit(BlocklistAIHistoryEvent::RemoveConversation {
                terminal_view_id,
                conversation_id,
            });
        }
    }

    /// Returns true if the conversation is live in any terminal view.
    pub fn is_conversation_live(&self, conversation_id: AIConversationId) -> bool {
        self.live_conversation_ids_for_terminal_view
            .values()
            .any(|conversation_ids| conversation_ids.contains(&conversation_id))
    }

    pub fn mark_terminal_view_as_ambient_agent_session_view(&mut self, terminal_view_id: EntityId) {
        self.ambient_agent_terminal_view_ids
            .insert(terminal_view_id);
    }

    pub fn mark_terminal_view_as_conversation_transcript_viewer(
        &mut self,
        terminal_view_id: EntityId,
    ) {
        self.conversation_transcript_viewer_terminal_view_ids
            .insert(terminal_view_id);
    }

    pub fn is_terminal_view_conversation_transcript_viewer(
        &self,
        terminal_view_id: EntityId,
    ) -> bool {
        self.conversation_transcript_viewer_terminal_view_ids
            .contains(&terminal_view_id)
    }

    /// Returns [`AIQueryHistory`]s from all sources: live conversations, cleared conversations,
    /// and persisted queries from conversations not loaded in memory.
    ///
    /// When `terminal_view_id` is provided, queries from that terminal view are categorized as
    /// `CurrentSession` and all others as `DifferentSession`. When `None`, all queries are
    /// categorized as `DifferentSession`.
    ///
    /// Ambient agent sessions are always excluded.
    pub(crate) fn all_ai_queries(
        &self,
        terminal_view_id: Option<EntityId>,
    ) -> impl Iterator<Item = AIQueryHistory> + '_ {
        // Collect all conversation IDs that are already in memory (live or cleared)
        // and build query vectors in the same loops
        let mut loaded_conversation_ids: HashSet<AIConversationId> = HashSet::new();

        let mut live_queries_vec = Vec::new();
        for (tv_id, conversation_ids) in self.live_conversation_ids_for_terminal_view.iter() {
            loaded_conversation_ids.extend(conversation_ids);

            // Skip shared ambient agent sessions
            if self.ambient_agent_terminal_view_ids.contains(tv_id) {
                continue;
            }

            let history_order = if terminal_view_id.is_some_and(|id| id == *tv_id) {
                HistoryOrder::CurrentSession
            } else {
                HistoryOrder::DifferentSession
            };

            for conversation_id in conversation_ids {
                if let Some(conversation) = self.conversations_by_id.get(conversation_id) {
                    for exchange in conversation.root_task_exchanges() {
                        if let Some(query) = ai_exchange_to_query_history(exchange, history_order) {
                            live_queries_vec.push(query);
                        }
                    }
                }
            }
        }

        let mut cleared_queries_vec = Vec::new();
        for (tv_id, conversation_ids) in self.cleared_conversation_ids_for_terminal_view.iter() {
            loaded_conversation_ids.extend(conversation_ids);

            let history_order = if terminal_view_id.is_some_and(|id| id == *tv_id) {
                HistoryOrder::CurrentSession
            } else {
                HistoryOrder::DifferentSession
            };

            for conversation_id in conversation_ids {
                if let Some(conversation) = self.conversations_by_id.get(conversation_id) {
                    for exchange in conversation.root_task_exchanges() {
                        if let Some(query) = ai_exchange_to_query_history(exchange, history_order) {
                            cleared_queries_vec.push(query);
                        }
                    }
                }
            }
        }

        // Add persisted queries from conversations not loaded in memory
        let persisted_queries_vec: Vec<_> = self
            .persisted_queries
            .iter()
            .filter(|persisted| !loaded_conversation_ids.contains(&persisted.conversation_id))
            .filter_map(|persisted| {
                persisted_ai_input_to_query_history(persisted, HistoryOrder::DifferentSession)
            })
            .collect();

        persisted_queries_vec
            .into_iter()
            .chain(cleared_queries_vec)
            .chain(live_queries_vec)
    }

    /// Returns the prompt-history candidates for NLD input classification, oldest-first
    /// (ascending). The matcher reverses this to iterate newest-first.
    ///
    /// Ported from the pin (`app/src/ai/blocklist/history_model.rs:2370-2373`, `02b53fcd8`) for
    /// #256, item 2. Consumed by `input_model.rs`'s `detect_and_set_input_type` (#312), gated on
    /// `FeatureFlag::NldPromptHistoryMatch`.
    ///
    /// The snapshot passed to `new` is now the real one: `sqlite.rs` derives `nld_prompts` from
    /// the same startup read that feeds up-arrow history (`process_ai_queries_for_nld_history_match`),
    /// and `lib.rs` hands it here. The previous note on this function claimed that read was
    /// "superseded by #336/#337/#331" -- it was not; those supersede items 1/3/4 of #256 and are
    /// all closed. The read was simply unported.
    ///
    /// **Still absent:** the pin's live-session append path (`append_session_prompt`), so within
    /// a single session a prompt the user typed after startup is not yet a candidate. Both the
    /// SQLite read and this function are gated behind `NldPromptHistoryMatch`, off by default
    /// (matching the pin's own default-off state), so that gap has no observable effect today.
    pub(crate) fn prompt_history_candidates(&self) -> Vec<PromptHistoryEntry> {
        self.prompt_history.clone()
    }

    /// Returns `Some` with the [`AIConversationId`] of the active conversation inside the
    /// [`crate::terminal::TerminalView`] with the given [`EntityId`] if there is one. Returns
    /// `None` otherwise.
    /// The active conversation is the one we're currently or have most recently streamed outputs for.
    /// If you want to check what conversation the next query will follow up in / what is selected in the input selector,
    /// use `context_model.selected_conversation_id` instead.
    pub(crate) fn active_conversation_id(
        &self,
        terminal_view_id: EntityId,
    ) -> Option<AIConversationId> {
        let active_conversation_id = self
            .active_conversation_for_terminal_view
            .get(&terminal_view_id)
            .copied()?;

        let conversation_ids_for_terminal_view = self
            .live_conversation_ids_for_terminal_view
            .get(&terminal_view_id)?;

        if !conversation_ids_for_terminal_view.contains(&active_conversation_id) {
            log::warn!(
                "The active conversation ID {active_conversation_id:?} was not found in the list of conversation IDs for terminal view {terminal_view_id:?}. Conversation IDs: {conversation_ids_for_terminal_view:?}"
            );
            return None;
        }

        Some(active_conversation_id)
    }

    /// Returns `Some` with the [`AIConversationId`] of the last conversation created for a given
    /// [`crate::terminal::TerminalView`] with the given [`EntityId`] if there is one. Returns
    /// `None` otherwise.
    #[cfg_attr(target_family = "wasm", allow(unused))]
    pub(crate) fn last_conversation_id(
        &self,
        terminal_view_id: EntityId,
    ) -> Option<AIConversationId> {
        self.live_conversation_ids_for_terminal_view
            .get(&terminal_view_id)?
            .last()
            .copied()
    }

    /// Set the hidden status of the exchange with the given ID.
    pub fn set_exchange_hidden_status(
        &mut self,
        terminal_view_id: EntityId,
        conversation_id: AIConversationId,
        exchange_id: AIAgentExchangeId,
        is_hidden: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) else {
            return;
        };
        conversation.set_is_exchange_hidden(exchange_id, is_hidden, terminal_view_id, ctx);
    }

    pub fn set_viewing_shared_session_for_conversation(
        &mut self,
        conversation_id: AIConversationId,
        is_viewing_shared_session: bool,
    ) {
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            conversation.set_is_viewing_shared_session(is_viewing_shared_session);
        }
    }

    pub fn set_has_code_review_opened_to_true(&mut self, conversation_id: AIConversationId) {
        if let Some(conversation) = self.conversations_by_id.get_mut(&conversation_id) {
            conversation.mark_code_review_as_opened();
        }
    }

    pub fn toggle_autoexecute_override(
        &mut self,
        conversation_id: &AIConversationId,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(conversation) = self.conversations_by_id.get_mut(conversation_id) else {
            return;
        };

        conversation.toggle_autoexecute_override();
        conversation.write_updated_conversation_state(ctx);
        ctx.emit(BlocklistAIHistoryEvent::UpdatedAutoexecuteOverride { terminal_view_id });
    }

    /// Truncates a conversation from the given exchange ID, removing all exchanges
    /// from that exchange onwards (inclusive). This is a lossy operation.
    ///
    /// Returns the set of exchange IDs that were removed.
    pub fn truncate_conversation_from_exchange(
        &mut self,
        conversation_id: AIConversationId,
        from_exchange_id: AIAgentExchangeId,
        ctx: &mut ModelContext<Self>,
    ) -> Result<HashSet<AIAgentExchangeId>, UpdateHistoryError> {
        let conversation = self
            .conversations_by_id
            .get_mut(&conversation_id)
            .ok_or(UpdateHistoryError::ConversationNotFound(conversation_id))?;

        let removed_exchange_ids = conversation.truncate_from_exchange(from_exchange_id, ctx)?;

        Ok(removed_exchange_ids)
    }

    /// Returns the latest exchange across all conversations in the terminal view.
    /// This is useful for determining if a specific exchange is the most recent one.
    /// Excludes passive code generation exchanges from consideration.
    pub fn latest_exchange_across_all_conversations(
        &self,
        terminal_view_id: EntityId,
    ) -> Option<&AIAgentExchange> {
        self.all_live_root_task_exchanges_for_terminal_view(terminal_view_id)
            .filter(|exchange| !exchange.has_passive_request())
            .max_by_key(|exchange| exchange.start_time)
    }

    /// Returns the conversation ID that contains the given exchange ID, if any.
    /// Searches through all conversations for a given terminal view.
    pub fn conversation_id_for_exchange(
        &self,
        exchange_id: AIAgentExchangeId,
        terminal_view_id: EntityId,
    ) -> Option<AIConversationId> {
        self.live_conversation_ids_for_terminal_view
            .get(&terminal_view_id)?
            .iter()
            .find(|conversation_id| {
                self.conversations_by_id
                    .get(conversation_id)
                    .is_some_and(|conversation| {
                        conversation.exchange_with_id(exchange_id).is_some()
                    })
            })
            .copied()
    }

    /// Returns local conversation metadata,
    /// (excluding conversations from ambient agent runs).
    pub fn get_local_conversations_metadata(
        &self,
    ) -> impl Iterator<Item = &AIConversationMetadata> {
        self.all_conversations_metadata
            .values()
            .filter(|m| !m.is_ambient_agent_conversation() && !m.is_child_agent_conversation())
    }

    /// Returns conversation metadata for a specific conversation ID.
    pub fn get_conversation_metadata(
        &self,
        conversation_id: &AIConversationId,
    ) -> Option<&AIConversationMetadata> {
        self.all_conversations_metadata.get(conversation_id)
    }

    /// Finds an AIConversationId by its server conversation token.
    ///
    /// O(1) lookup via `server_token_to_conversation_id`, which is maintained
    /// alongside every mutation of `conversations_by_id` /
    /// `all_conversations_metadata` that involves a token. Used to look up
    /// conversations for ambient agent tasks, which store the server token
    /// but not the AIConversationId.
    pub fn find_conversation_id_by_server_token(
        &self,
        server_token: &ServerConversationToken,
    ) -> Option<AIConversationId> {
        if let Some(id) = self.server_token_to_conversation_id.get(server_token) {
            return Some(*id);
        }

        // A token miss is the expected outcome whenever a task references a
        // conversation this client hasn't loaded (shared-session tasks from
        // other users, pre-sync state).
        log::debug!(
            "No conversation found for server token: {}",
            server_token.as_str()
        );
        None
    }

    /// Mark conversations as historical
    /// Historical conversations consist of non-live conversations that were read from the disk or server on startup,
    /// and conversations (recorded here) that were live this session but have now been cleared.
    pub fn mark_conversations_historical_for_terminal_view(&mut self, terminal_view_id: EntityId) {
        if self.is_terminal_view_conversation_transcript_viewer(terminal_view_id) {
            // We don't mark conversation transcript viewer conversations as historical,
            // as they are stored separately and should not be persisted/displayed as regular user conversations.
            return;
        }

        // There's a slight concern here that the conversations we're preserving might not have persisted successfully
        // because of some unexpected error. Attempting to then restore these conversations would lead to unexpected behavior.
        // In the future it might be worthwhile to check that these conversations exist in the database before marking them as historical,
        // but for now this is an edge case that we don't need to worry about too much.
        let conversations_to_mark_historical: Vec<AIConversationMetadata> = self
            .all_live_conversations_for_terminal_view(terminal_view_id)
            .filter_map(|conversation| {
                let conversation_id = conversation.id();
                if !self.conversations_by_id.contains_key(&conversation_id)
                    || conversation.should_exclude_from_navigation()
                    || !blocklist_filter::conversation_would_render_in_blocklist(conversation)
                {
                    return None;
                }

                Some(conversation.into())
            })
            .collect();

        for metadata in conversations_to_mark_historical {
            if let Some(token) = &metadata.server_conversation_token {
                self.server_token_to_conversation_id
                    .insert(token.clone(), metadata.id);
            }
            self.all_conversations_metadata
                .insert(metadata.id, metadata);
        }
    }

    /// Inserts a conversation into memory by reconstructing exchanges from tasks.
    /// We use this when forking a conversation to ensure that the forked conversation
    /// is immediately available in memory before we try to restore it in a new tab.
    pub fn insert_forked_conversation_from_tasks(
        &mut self,
        conversation_id: AIConversationId,
        tasks: Vec<warp_multi_agent_api::Task>,
        conversation_data: AgentConversationData,
    ) -> anyhow::Result<AIConversation> {
        let mut conversation =
            AIConversation::new_restored(conversation_id, tasks, Some(conversation_data))?;

        // Assign fresh exchange IDs so persisted blocks do not collide.
        conversation.reassign_exchange_ids();

        if let Some(token) = conversation.server_conversation_token() {
            self.server_token_to_conversation_id
                .insert(token.clone(), conversation_id);
        }

        self.conversations_by_id
            .insert(conversation_id, conversation.clone());

        // This is harmless if we're opening the conversation immediately, but ensures it's in the conversation list right away if we fork in the background.
        let metadata = AIConversationMetadata::from(&conversation);
        if let Some(token) = &metadata.server_conversation_token {
            self.server_token_to_conversation_id
                .insert(token.clone(), conversation_id);
        }
        self.all_conversations_metadata
            .insert(conversation_id, metadata);

        Ok(conversation)
    }

    /// Clears all stored conversation-related data in memory.
    /// This is used when logging out to ensure no AI history persists across users.
    pub(crate) fn reset(&mut self) {
        self.live_conversation_ids_for_terminal_view.clear();
        self.cleared_conversation_ids_for_terminal_view.clear();
        self.conversations_by_id.clear();
        self.active_conversation_for_terminal_view.clear();
        self.ambient_agent_terminal_view_ids.clear();
        self.conversation_transcript_viewer_terminal_view_ids
            .clear();
        self.persisted_queries.clear();
        self.all_conversations_metadata.clear();
        self.agent_id_to_conversation_id.clear();
        self.server_token_to_conversation_id.clear();
    }
}

/// Returns the key to use in `agent_id_to_conversation_id` for the given
/// conversation.
fn agent_id_key(conversation: &AIConversation) -> Option<String> {
    conversation.agent_link_id()
}

/// The startup counterpart of [`agent_id_key`]: the key to use in
/// `agent_id_to_conversation_id` for a persisted row that has not been
/// hydrated into an [`AIConversation`] yet. The persisted `run_id` is the
/// orchestration agent identifier a child's `parent_agent_id` points at;
/// server tokens are indexed separately in `server_token_to_conversation_id`
/// (and `conversation_id_for_agent_id` already falls back to it). Ported from
/// `42effe840:app/src/ai/blocklist/history_model.rs:2896`.
fn agent_id_key_from_persisted_data(conversation_data: &AgentConversationData) -> Option<&str> {
    conversation_data.run_id.as_deref()
}

/// Whether an `UpdatedConversationStatus` event represents a restoration
/// (the conversation was re-loaded for a terminal view; the underlying
/// `ConversationStatus` did not change) or a real status set, in which case
/// the previous status is included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationStatusUpdate {
    Restored,
    Changed { prev_status: ConversationStatus },
}

#[derive(Clone, Debug)]
pub enum BlocklistAIHistoryEvent {
    /// A new conversation was started.
    StartedNewConversation {
        new_conversation_id: AIConversationId,
        terminal_view_id: EntityId,
    },

    /// Emitted when a conversation title changes (e.g. via a rename).
    UpdatedConversationTitle {
        terminal_view_id: Option<EntityId>,
        conversation_id: AIConversationId,
        title: String,
    },

    CreatedSubtask {
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        task_id: TaskId,
    },

    /// Emitted when the optimistically created task is upgraded to a confirmed task upon
    /// receiving a CreateTask client action.
    UpgradedTask {
        optimistic_id: TaskId,
        confirmed_task_id: TaskId,
        terminal_view_id: EntityId,
    },

    AppendedExchange {
        exchange_id: AIAgentExchangeId,
        task_id: TaskId,
        terminal_view_id: EntityId,
        conversation_id: AIConversationId,
        is_hidden: bool,

        // Populated if this exchange is appended as a result of an in-flight API request.
        response_stream_id: Option<ResponseStreamId>,
    },

    ReassignedExchange {
        exchange_id: AIAgentExchangeId,
        terminal_view_id: EntityId,
        new_task_id: TaskId,
        new_conversation_id: AIConversationId,
    },

    /// Includes the terminal view's [`EntityId`] so we can disambiguate the source of the event
    /// because this [`BlocklistAIHistoryModel`] is global.
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    UpdatedStreamingExchange {
        exchange_id: AIAgentExchangeId,
        terminal_view_id: EntityId,
        conversation_id: AIConversationId,
        is_hidden: bool,
    },

    UpdatedConversationStatus {
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
        /// Distinguishes a restoration from a real status set.
        update: ConversationStatusUpdate,
        /// The conversation's status after this update.
        new_status: ConversationStatus,
    },

    /// The active conversation was set to another conversation in the history.
    SetActiveConversation {
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
    },

    /// `conversation_id` is no longer marked as active for the given terminal view.
    ClearedActiveConversation {
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
    },

    /// A conversation was removed from `previous_terminal_view_id`'s live-conversation
    /// list because it was claimed by `new_terminal_view_id` (#418). The previous owner
    /// should drop any rendered blocks it holds for the conversation; the new owner
    /// handles rendering itself via the ordinary `AppendedExchange`/restore paths.
    ConversationTransferredBetweenTerminalViews {
        conversation_id: AIConversationId,
        previous_terminal_view_id: EntityId,
        new_terminal_view_id: EntityId,
    },

    ClearedConversationsInTerminalView {
        terminal_view_id: EntityId,
        active_conversation_id: Option<AIConversationId>,
    },

    UpdatedTodoList {
        terminal_view_id: EntityId,
    },

    UpdatedAutoexecuteOverride {
        terminal_view_id: EntityId,
    },

    /// Emitted when a conversation is split into two (on suggest starting new conversation)
    SplitConversation {
        terminal_view_id: EntityId,
        old_conversation_id: AIConversationId,
        new_conversation_id: AIConversationId,
    },

    /// This is emitted when an ephemeral/abandoned conversation is cleaned up
    /// (e.g. empty conversations the user never used, rejected passive code suggestions).
    RemoveConversation {
        terminal_view_id: EntityId,
        conversation_id: AIConversationId,
    },

    /// This is emitted when a user explicitly deletes an existing conversation.
    /// `terminal_view_id` is `None` when deleting a historical-only conversation that
    /// is not currently bound to any terminal view (e.g. right-click delete from the
    /// conversation list for a session that was never opened in this run).
    DeletedConversation {
        terminal_view_id: Option<EntityId>,
        conversation_id: AIConversationId,
        conversation_title: Option<String>,
    },

    /// Emitted when conversations are restored in a terminal view.
    RestoredConversations {
        terminal_view_id: EntityId,
        conversation_ids: Vec<AIConversationId>,
    },

    /// Emitted when conversation metadata is updated.
    /// `terminal_view_id` is None when updating historical-only conversations.
    UpdatedConversationMetadata {
        terminal_view_id: Option<EntityId>,
        conversation_id: AIConversationId,
    },

    /// Emitted when conversation artifacts are updated (plans, PRs, etc.)
    UpdatedConversationArtifacts {
        terminal_view_id: EntityId,
        conversation_id: AIConversationId,
        artifact: Artifact,
    },

    /// Emitted when a conversation first receives its server-assigned conversation token
    /// during StreamInit. Retained for child-agent and restored conversation linking.
    ConversationAgentIdAssigned {
        conversation_id: AIConversationId,
        terminal_view_id: EntityId,
    },
}

impl BlocklistAIHistoryEvent {
    /// Returns the terminal view ID associated with this event, if any.
    /// Returns `None` for events that apply globally (e.g., historical conversation metadata updates).
    pub fn terminal_view_id(&self) -> Option<EntityId> {
        match self {
            BlocklistAIHistoryEvent::StartedNewConversation {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::AppendedExchange {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::UpdatedStreamingExchange {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::UpdatedConversationStatus {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::SetActiveConversation {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::ClearedActiveConversation {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::ClearedConversationsInTerminalView {
                terminal_view_id,
                ..
            }
            | BlocklistAIHistoryEvent::ReassignedExchange {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::UpdatedTodoList {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::UpdatedAutoexecuteOverride {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::SplitConversation {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::RemoveConversation {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::CreatedSubtask {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::RestoredConversations {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::UpgradedTask {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::UpdatedConversationArtifacts {
                terminal_view_id, ..
            }
            | BlocklistAIHistoryEvent::ConversationAgentIdAssigned {
                terminal_view_id, ..
            } => Some(*terminal_view_id),
            // UpdatedConversationMetadata can have None when updating historical-only conversations
            BlocklistAIHistoryEvent::UpdatedConversationMetadata {
                terminal_view_id, ..
            } => *terminal_view_id,
            // DeletedConversation can have None when deleting historical-only conversations
            BlocklistAIHistoryEvent::DeletedConversation {
                terminal_view_id, ..
            } => *terminal_view_id,
            // UpdatedConversationTitle can have None when the conversation is not
            // live in any terminal view (e.g. renamed from history).
            BlocklistAIHistoryEvent::UpdatedConversationTitle {
                terminal_view_id, ..
            } => *terminal_view_id,
            // No single "the" terminal view: this event names both a previous and
            // a new owner. Handlers inspect `previous_terminal_view_id` directly.
            BlocklistAIHistoryEvent::ConversationTransferredBetweenTerminalViews { .. } => None,
        }
    }
}

impl Entity for BlocklistAIHistoryModel {
    type Event = BlocklistAIHistoryEvent;
}

impl SingletonEntity for BlocklistAIHistoryModel {}

/// Helper struct for showing AI history to the user. Guarantees that there is a user query and
/// contains less data than [`AIAgentExchange`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIQueryHistory {
    /// The input originating from the user.
    pub query_text: String,

    /// The time the input was sent.
    pub start_time: DateTime<Local>,

    /// The status of the output streaming from the AI API.
    pub output_status: AIQueryHistoryOutputStatus,

    /// The working directory when the AI query was submitted.
    pub working_directory: Option<String>,

    /// The ordering category for this query in history.
    pub history_order: HistoryOrder,
}

impl AIQueryHistory {
    /// Creates a new [`AIQueryHistory`] for testing.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        query_text: &str,
        start_time: DateTime<Local>,
        history_order: HistoryOrder,
    ) -> Self {
        Self {
            query_text: query_text.to_owned(),
            start_time,
            output_status: AIQueryHistoryOutputStatus::Pending,
            working_directory: None,
            history_order,
        }
    }
}

fn ai_exchange_to_query_history(
    value: &AIAgentExchange,
    history_order: HistoryOrder,
) -> Option<AIQueryHistory> {
    let query = value.input.iter().find_map(AIAgentInput::user_query)?;

    Some(AIQueryHistory {
        query_text: query,
        start_time: value.start_time,
        output_status: AIQueryHistoryOutputStatus::from(&value.output_status),
        working_directory: value.working_directory.clone(),
        history_order,
    })
}

fn persisted_ai_input_to_query_history(
    value: &PersistedAIInput,
    history_order: HistoryOrder,
) -> Option<AIQueryHistory> {
    // Extract the query text from the first Query input
    let query_text = value
        .inputs
        .iter()
        .map(|input| match input {
            PersistedAIInputType::Query { text, .. } => Some(text.clone()),
        })
        .next()
        .flatten()?;

    Some(AIQueryHistory {
        query_text,
        start_time: value.start_ts,
        output_status: value.output_status.clone(),
        working_directory: value.working_directory.clone(),
        history_order,
    })
}

/// Status of output streaming from the AI API.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AIQueryHistoryOutputStatus {
    /// We are waiting to or are currently streaming output.
    Pending,
    /// The user manually cancelled output streaming.
    Cancelled,
    /// Output streaming failed.
    Failed,
    /// Output streaming completed successfully.
    Completed,
}

impl AIQueryHistoryOutputStatus {
    /// Returns a string representation of the output status.
    pub(crate) fn display_text(&self) -> String {
        match self {
            AIQueryHistoryOutputStatus::Completed => {
                crate::t!("ai-history-completed-successfully")
            }
            AIQueryHistoryOutputStatus::Pending => crate::t!("ai-history-pending"),
            AIQueryHistoryOutputStatus::Cancelled => crate::t!("ai-history-cancelled-by-user"),
            AIQueryHistoryOutputStatus::Failed => crate::t!("common-failed"),
        }
    }

    pub(crate) fn icon(&self) -> Icon {
        match self {
            AIQueryHistoryOutputStatus::Completed => Icon::Check,
            AIQueryHistoryOutputStatus::Pending => Icon::Loading,
            AIQueryHistoryOutputStatus::Cancelled => Icon::SlashCircle,
            AIQueryHistoryOutputStatus::Failed => Icon::AlertTriangle,
        }
    }
}

impl From<&AIAgentOutputStatus> for AIQueryHistoryOutputStatus {
    fn from(status: &AIAgentOutputStatus) -> Self {
        match status {
            AIAgentOutputStatus::Streaming { .. } => Self::Pending,
            AIAgentOutputStatus::Finished {
                finished_output, ..
            } => match finished_output {
                FinishedAIAgentOutput::Cancelled { .. } => Self::Cancelled,
                FinishedAIAgentOutput::Error { .. } => Self::Failed,
                FinishedAIAgentOutput::Success { .. } => Self::Completed,
            },
        }
    }
}

/// Updates the given tasks, which are presumed to be clones of tasks from a source conversation to be
/// used to back a fork or copy of the source conversation.
///
/// Reassigns new task IDs to each forked task to ensure task IDs remain globally unique and updates
/// description of the root task, prepending it with the given prefix.
fn update_forked_task_properties(
    tasks: Vec<warp_multi_agent_api::Task>,
    prefix: &str,
    preserve_task_ids: bool,
    title_override: Option<&str>,
) -> Vec<warp_multi_agent_api::Task> {
    let root_description = |current: &str| match title_override {
        Some(title) => title.to_owned(),
        None => format!("{prefix}{current}"),
    };

    if preserve_task_ids {
        return tasks
            .into_iter()
            .map(|mut t| {
                let is_root = t
                    .dependencies
                    .as_ref()
                    .map(|deps| deps.parent_task_id.is_empty())
                    .unwrap_or(true);
                if is_root {
                    t.description = root_description(&t.description);
                }
                t
            })
            .collect();
    }

    let mut old_to_new_task_ids = HashMap::new();
    fn get_new_task_id(new_ids: &mut HashMap<String, String>, old_task_id: &str) -> String {
        new_ids
            .entry(old_task_id.to_owned())
            .or_insert_with(|| Uuid::new_v4().to_string())
            .clone()
    }

    tasks
        .into_iter()
        .map(|mut t| {
            let new_id = get_new_task_id(&mut old_to_new_task_ids, &t.id);
            // Update task id to avoid duplicate tasks across conversations and ensure
            // all messages reference the new task id.
            t.id = new_id.clone();
            for message in &mut t.messages {
                message.task_id = new_id.clone();
                if let Some(subagent) = message.tool_call_mut().and_then(|tc| tc.subagent_mut()) {
                    subagent.task_id =
                        get_new_task_id(&mut old_to_new_task_ids, &subagent.task_id).clone();
                }
            }
            if let Some(deps) = t
                .dependencies
                .as_mut()
                .filter(|deps| !deps.parent_task_id.is_empty())
            {
                deps.parent_task_id =
                    get_new_task_id(&mut old_to_new_task_ids, &deps.parent_task_id).clone();
            } else {
                t.description = root_description(&t.description);
            }
            t
        })
        .collect()
}

fn byop_fork_repair_state_json(
    source_tasks: &[warp_multi_agent_api::Task],
    forked_tasks: &[warp_multi_agent_api::Task],
    source_repair_state: &RepairStateStatus,
    exchange_id: Option<String>,
) -> Option<String> {
    let records = collect_byop_fork_repair_records(
        source_tasks,
        forked_tasks,
        source_repair_state,
        exchange_id,
    );
    RepairStateStatus::Valid(RepairState::new(records)).to_sidecar_json()
}

fn collect_byop_fork_repair_records(
    source_tasks: &[warp_multi_agent_api::Task],
    forked_tasks: &[warp_multi_agent_api::Task],
    source_repair_state: &RepairStateStatus,
    exchange_id: Option<String>,
) -> Vec<RepairRecord> {
    let source_tool_call_keys = byop_tool_call_keys_by_message_id(source_tasks);
    let forked_tool_call_keys = byop_tool_call_keys_by_message_id(forked_tasks);
    let source_result_message_ids = byop_result_message_ids_by_tool_call_key(source_tasks);
    let retained_message_ids = byop_message_ids(forked_tasks);

    let mut source_to_fork_key = HashMap::new();
    for (message_id, fork_key) in &forked_tool_call_keys {
        let Some(source_key) = source_tool_call_keys.get(message_id) else {
            continue;
        };
        source_to_fork_key.insert(source_key.clone(), fork_key.clone());
    }

    let mut records = Vec::new();
    let mut seen_keys = HashSet::new();
    for (source_key, fork_key) in &source_to_fork_key {
        let result_ids = source_result_message_ids
            .get(source_key)
            .cloned()
            .unwrap_or_default();
        if result_ids.is_empty()
            || result_ids
                .iter()
                .any(|id| retained_message_ids.contains(id))
        {
            continue;
        }

        let mut record = RepairRecord::new(RepairSource::ForkedHistory, fork_key.clone());
        record.exchange_id = exchange_id.clone();
        push_unique_repair_record(&mut records, &mut seen_keys, record);
    }

    for record in source_repair_state.repair_records() {
        let Some(fork_key) = source_to_fork_key.get(&record.key) else {
            continue;
        };
        if source_result_message_ids
            .get(&record.key)
            .is_some_and(|ids| ids.iter().any(|id| retained_message_ids.contains(id)))
        {
            continue;
        }

        let mut translated = RepairRecord::new(record.source, fork_key.clone());
        translated.fork_point_message_id = record.fork_point_message_id.clone();
        translated.exchange_id = record.exchange_id.clone().or_else(|| exchange_id.clone());
        push_unique_repair_record(&mut records, &mut seen_keys, translated);
    }

    records
}

fn push_unique_repair_record(
    records: &mut Vec<RepairRecord>,
    seen_keys: &mut HashSet<ToolCallKey>,
    record: RepairRecord,
) {
    if seen_keys.insert(record.key.clone()) {
        records.push(record);
    }
}

fn byop_message_ids(tasks: &[warp_multi_agent_api::Task]) -> HashSet<String> {
    tasks
        .iter()
        .flat_map(|task| task.messages.iter().map(|message| message.id.clone()))
        .collect()
}

fn byop_result_message_ids_by_tool_call_key(
    tasks: &[warp_multi_agent_api::Task],
) -> HashMap<ToolCallKey, HashSet<String>> {
    let mut result_ids = HashMap::new();
    for task in tasks {
        let mut active_task_id: Option<String> = None;
        let mut active_assistant_message_id: Option<String> = None;
        let mut active_tool_call_ids: Vec<String> = Vec::new();
        let mut active_saw_tool_result = false;
        for message in &task.messages {
            match message.message.as_ref() {
                Some(warp_multi_agent_api::message::Message::ToolCall(tool_call)) => {
                    if tool_call.subagent().is_some() {
                        continue;
                    }

                    if active_task_id
                        .as_deref()
                        .is_some_and(|task_id| task_id != message.task_id)
                    {
                        active_assistant_message_id = None;
                        active_tool_call_ids.clear();
                        active_saw_tool_result = false;
                    }

                    if active_saw_tool_result {
                        active_task_id = None;
                        active_assistant_message_id = None;
                        active_tool_call_ids.clear();
                        active_saw_tool_result = false;
                    }

                    if active_tool_call_ids.is_empty() {
                        active_task_id = Some(message.task_id.clone());
                        active_assistant_message_id = Some(message.id.clone());
                    }
                    active_tool_call_ids.push(tool_call.tool_call_id.clone());
                }
                Some(warp_multi_agent_api::message::Message::ToolCallResult(result)) => {
                    let Some(assistant_message_id) = active_assistant_message_id.as_ref() else {
                        continue;
                    };
                    if active_task_id.as_deref() != Some(message.task_id.as_str()) {
                        continue;
                    }
                    let matching_count = active_tool_call_ids
                        .iter()
                        .filter(|tool_call_id| *tool_call_id == &result.tool_call_id)
                        .count();
                    if matching_count != 1 {
                        continue;
                    }

                    result_ids
                        .entry(ToolCallKey::new(
                            message.task_id.clone(),
                            assistant_message_id.clone(),
                            result.tool_call_id.clone(),
                        ))
                        .or_insert_with(HashSet::new)
                        .insert(message.id.clone());
                    active_saw_tool_result = true;
                }
                Some(warp_multi_agent_api::message::Message::UserQuery(_))
                | Some(warp_multi_agent_api::message::Message::AgentOutput(_)) => {
                    active_task_id = None;
                    active_assistant_message_id = None;
                    active_tool_call_ids.clear();
                    active_saw_tool_result = false;
                }
                Some(warp_multi_agent_api::message::Message::AgentReasoning(_))
                | Some(warp_multi_agent_api::message::Message::ServerEvent(_))
                | Some(warp_multi_agent_api::message::Message::SystemQuery(_))
                | Some(warp_multi_agent_api::message::Message::UpdateTodos(_))
                | Some(warp_multi_agent_api::message::Message::Summarization(_))
                | Some(warp_multi_agent_api::message::Message::CodeReview(_))
                | Some(warp_multi_agent_api::message::Message::UpdateReviewComments(_))
                | Some(warp_multi_agent_api::message::Message::WebSearch(_))
                | Some(warp_multi_agent_api::message::Message::WebFetch(_))
                | Some(warp_multi_agent_api::message::Message::DebugOutput(_))
                | Some(warp_multi_agent_api::message::Message::ArtifactEvent(_))
                | Some(warp_multi_agent_api::message::Message::InvokeSkill(_))
                | Some(warp_multi_agent_api::message::Message::MessagesReceivedFromAgents(_))
                | Some(warp_multi_agent_api::message::Message::ModelUsed(_))
                | Some(warp_multi_agent_api::message::Message::EventsFromAgents(_))
                | Some(warp_multi_agent_api::message::Message::PassiveSuggestionResult(_))
                // Orchestration is server-side and unsupported here; see #11.
                | Some(warp_multi_agent_api::message::Message::OrchestrationConfigSnapshot(_))
                | None => {}
            }
        }
    }
    result_ids
}

fn byop_tool_call_keys_by_message_id(
    tasks: &[warp_multi_agent_api::Task],
) -> HashMap<String, ToolCallKey> {
    let mut keys = HashMap::new();
    for task in tasks {
        let mut pending_task_id: Option<String> = None;
        let mut pending_assistant_message_id: Option<String> = None;
        for message in &task.messages {
            match message.message.as_ref() {
                Some(warp_multi_agent_api::message::Message::ToolCall(tool_call)) => {
                    if tool_call.subagent().is_some() {
                        continue;
                    }

                    if pending_task_id
                        .as_deref()
                        .is_some_and(|task_id| task_id != message.task_id)
                    {
                        pending_assistant_message_id = None;
                    }

                    let assistant_message_id = pending_assistant_message_id
                        .get_or_insert_with(|| message.id.clone())
                        .clone();
                    pending_task_id = Some(message.task_id.clone());
                    keys.insert(
                        message.id.clone(),
                        ToolCallKey::new(
                            message.task_id.clone(),
                            assistant_message_id,
                            tool_call.tool_call_id.clone(),
                        ),
                    );
                }
                Some(warp_multi_agent_api::message::Message::UserQuery(_))
                | Some(warp_multi_agent_api::message::Message::AgentOutput(_))
                | Some(warp_multi_agent_api::message::Message::ToolCallResult(_)) => {
                    pending_task_id = None;
                    pending_assistant_message_id = None;
                }
                Some(warp_multi_agent_api::message::Message::AgentReasoning(_))
                | Some(warp_multi_agent_api::message::Message::ServerEvent(_))
                | Some(warp_multi_agent_api::message::Message::SystemQuery(_))
                | Some(warp_multi_agent_api::message::Message::UpdateTodos(_))
                | Some(warp_multi_agent_api::message::Message::Summarization(_))
                | Some(warp_multi_agent_api::message::Message::CodeReview(_))
                | Some(warp_multi_agent_api::message::Message::UpdateReviewComments(_))
                | Some(warp_multi_agent_api::message::Message::WebSearch(_))
                | Some(warp_multi_agent_api::message::Message::WebFetch(_))
                | Some(warp_multi_agent_api::message::Message::DebugOutput(_))
                | Some(warp_multi_agent_api::message::Message::ArtifactEvent(_))
                | Some(warp_multi_agent_api::message::Message::InvokeSkill(_))
                | Some(warp_multi_agent_api::message::Message::MessagesReceivedFromAgents(_))
                | Some(warp_multi_agent_api::message::Message::ModelUsed(_))
                | Some(warp_multi_agent_api::message::Message::EventsFromAgents(_))
                | Some(warp_multi_agent_api::message::Message::PassiveSuggestionResult(_))
                // Orchestration is server-side and unsupported here; see #11.
                | Some(warp_multi_agent_api::message::Message::OrchestrationConfigSnapshot(_))
                | None => {}
            }
        }
    }
    keys
}

/// The default prefix used when forking a conversation.
pub const FORK_PREFIX: &str = "(Fork) ";

/// The prefix used when saving a conversation before a rewind operation.
pub const PRE_REWIND_PREFIX: &str = "(Pre-Rewind) ";

#[cfg(test)]
#[path = "history_model_test.rs"]
mod tests;

/// Pairs up client tool_calls that an exact fork left hanging at the fork point.
///
/// An exact fork keeps everything through the fork-point exchange and drops the
/// rest, which can cut between a tool_call and its tool_call_result. Four cases
/// arise, and only the first two get a result:
///
/// 1. The result exists in a dropped later turn -- pull the REAL result forward,
///    so the fork keeps the work the tool actually did.
/// 2. No result exists anywhere (the call was still in flight when the fork was
///    taken) -- synthesize a `Cancel` immediately after the call, carrying the
///    call's `request_id`.
/// 3. A SERVER tool_call is deliberately left unresolved. Synthesizing a result
///    for one pops an agent off the server's run stack when the conversation is
///    restored, corrupting the fork in a way that is far worse than a dangling
///    call.
/// 4. A tool_call that was ALREADY dangling before the fork point is reproduced
///    as-is. It is pre-existing history, not damage this fork caused, and
///    "repairing" it would rewrite what the user actually saw.
fn reconcile_fork_point_tool_calls(
    truncated_tasks: &mut [warp_multi_agent_api::Task],
    source_tasks: &[warp_multi_agent_api::Task],
    root_task_id: &TaskId,
    fork_point_message_ids: &HashSet<MessageId>,
) {
    use crate::ai::agent::task::helper::MessageExt;

    let Some(root) = truncated_tasks
        .iter_mut()
        .find(|t| TaskId::new(t.id.clone()) == *root_task_id)
    else {
        return;
    };
    let source_root = source_tasks
        .iter()
        .find(|t| TaskId::new(t.id.clone()) == *root_task_id);

    let already_paired: HashSet<String> = root
        .messages
        .iter()
        .filter_map(|m| m.tool_call_result().map(|r| r.tool_call_id.clone()))
        .collect();

    // Collect first, mutate after: inserting while iterating would invalidate the
    // indices this depends on.
    let mut insertions: Vec<(usize, warp_multi_agent_api::Message)> = Vec::new();
    for (index, message) in root.messages.iter().enumerate() {
        // Case 4: only the fork-point exchange is reconciled.
        if !fork_point_message_ids.contains(&MessageId::new(message.id.clone())) {
            continue;
        }
        let Some(tool_call) = message.tool_call() else {
            continue;
        };
        // Case 3: never synthesize for a server tool_call.
        if matches!(
            tool_call.tool,
            Some(warp_multi_agent_api::message::tool_call::Tool::Server(_))
        ) {
            continue;
        }
        if already_paired.contains(&tool_call.tool_call_id) {
            continue;
        }

        // Case 1: the real result, from a turn this fork dropped.
        let real_result = source_root.and_then(|source_root| {
            source_root
                .messages
                .iter()
                .find(|m| {
                    m.tool_call_result()
                        .is_some_and(|r| r.tool_call_id == tool_call.tool_call_id)
                })
                .cloned()
        });

        let result_message = real_result.unwrap_or_else(|| {
            // Case 2: cloned from the call so task_id, request_id and any future
            // envelope fields carry over; only identity and payload change.
            let mut cancel = message.clone();
            cancel.id = format!("{}_cancel", message.id);
            cancel.message = Some(warp_multi_agent_api::message::Message::ToolCallResult(
                warp_multi_agent_api::message::ToolCallResult {
                    tool_call_id: tool_call.tool_call_id.clone(),
                    context: None,
                    result: Some(
                        warp_multi_agent_api::message::tool_call_result::Result::Cancel(()),
                    ),
                },
            ));
            cancel
        });
        insertions.push((index + 1, result_message));
    }

    for (shift, (index, message)) in insertions.into_iter().enumerate() {
        root.messages.insert(index + shift, message);
    }
}

