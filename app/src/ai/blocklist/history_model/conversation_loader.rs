//! This module contains functions for loading conversation data from the local database.

use std::collections::HashMap;
use std::future::Future;

use chrono::TimeZone;
use futures::FutureExt;
use itertools::Itertools as _;
use persistence::model::AgentConversationRecord;

use crate::ai::agent::api::ServerConversationToken;
use crate::ai::agent::conversation::{
    AIConversation, AIConversationId, ServerAIConversationMetadata,
};
use crate::persistence::model::{AgentConversation, AgentConversationData, AgentConversationSummary};
use crate::terminal::model::block::SerializedBlock;

#[cfg(feature = "local_fs")]
use crate::persistence::agent::read_agent_conversation_by_id;

use super::{AIConversationMetadata, BlocklistAIHistoryModel, MAX_HISTORICAL_CONVERSATIONS};

/// A conversation transcript from a CLI agent harness (e.g. Claude Code).
#[derive(Debug, Clone)]
pub struct CLIAgentConversation {
    /// Server metadata about this conversation.
    pub metadata: ServerAIConversationMetadata,
    /// A snapshot of the final agent TUI state.
    pub block: SerializedBlock,
}

/// Representation of loaded local conversation data.
///
/// The concrete format depends on the agent harness that produced the conversation.
pub enum LoadedConversationData {
    /// A conversation produced by the Oz harness, restorable into the [`AIConversation`] data model.
    Oz(Box<AIConversation>),
    /// A conversation produced by an external CLI agent harness.
    CLIAgent(Box<CLIAgentConversation>),
}

/// Converts an `AgentConversation` from the database to an `AIConversation`.
/// This utility function extracts the conversion logic that was originally embedded
/// in the terminal view restoration process.
#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
pub fn convert_persisted_conversation_to_ai_conversation(
    persisted_conversation: AgentConversation,
) -> Option<AIConversation> {
    convert_persisted_conversation_to_ai_conversation_with_metadata(persisted_conversation)
}

/// Enhanced version of the conversion function with additional metadata.
/// This version supports the full feature set needed by terminal view restoration.
pub fn convert_persisted_conversation_to_ai_conversation_with_metadata(
    persisted_conversation: AgentConversation,
) -> Option<AIConversation> {
    let AgentConversation {
        tasks,
        conversation:
            AgentConversationRecord {
                conversation_id,
                conversation_data,
                last_modified_at,
                ..
            },
    } = persisted_conversation;

    let conversation_id = match AIConversationId::try_from(conversation_id) {
        Ok(id) => id,
        Err(e) => {
            log::warn!("Failed to convert conversation ID: {e:?}");
            return None;
        }
    };

    let conversation_data = serde_json::from_str::<AgentConversationData>(&conversation_data).ok();

    match AIConversation::new_restored(conversation_id, tasks, conversation_data) {
        Ok(mut conversation) => {
            // Old messages in a persisted Task may lack CurrentTime/timestamp,
            // falling back to the Unix epoch when the exchange is restored. SQLite's
            // row-level update time is a reliable fallback for when this
            // conversation was last written.
            let fallback_timestamp = chrono::Local.from_utc_datetime(&last_modified_at);
            conversation.repair_default_restored_exchange_timestamps(fallback_timestamp);
            Some(conversation)
        }
        Err(e) => {
            log::debug!("Skipping persisted conversation (legacy/incomplete): {e:?}");
            None
        }
    }
}

/// Boxes a future with the right type for the platform.
/// On WASM, futures must not implement Send.
fn box_future<F>(f: F) -> warpui::r#async::BoxFuture<'static, Option<LoadedConversationData>>
where
    F: Future<Output = Option<LoadedConversationData>> + warpui::r#async::Spawnable,
{
    cfg_if::cfg_if! {
        if #[cfg(target_family = "wasm")] {
            f.boxed_local()
        } else {
            f.boxed()
        }
    }
}

impl BlocklistAIHistoryModel {
    /// Loads conversation data from memory or the local database.
    ///
    /// This method automatically determines whether to load from memory or local storage:
    /// - If the conversation is already in memory, returns it immediately
    /// - If has_local_data is true, loads from the local database synchronously
    ///
    /// Note: This does NOT insert the conversation into memory. Callers are responsible
    /// for inserting the loaded conversation if needed.
    pub fn load_conversation_data(
        &self,
        conversation_id: AIConversationId,
    ) -> warpui::r#async::BoxFuture<'static, Option<LoadedConversationData>> {
        // First check if the conversation is already in memory
        if let Some(conversation) = self.conversations_by_id.get(&conversation_id) {
            return box_future(futures::future::ready(Some(LoadedConversationData::Oz(
                Box::new(conversation.clone()),
            ))));
        }

        // Check metadata to determine the source
        let Some(metadata) = self
            .all_conversations_metadata
            .get(&conversation_id)
            .cloned()
        else {
            log::warn!("No metadata found for conversation {conversation_id}");
            return box_future(futures::future::ready(None));
        };

        if metadata.has_local_data {
            // Load from local database synchronously
            let result = self
                .load_conversation_from_db(&conversation_id)
                .map(|c| LoadedConversationData::Oz(Box::new(c)));
            box_future(futures::future::ready(result))
        } else {
            log::warn!("Cannot load conversation {conversation_id}: no local data");
            box_future(futures::future::ready(None))
        }
    }

    /// Loads a conversation by its server token.
    ///
    /// BYOP has no cloud server and no server conversation tokens, so there is
    /// nothing to load by token — always resolves to `None`. (warp loads these
    /// from the cloud server; the `warp_tui` `Server` restore target never fires
    /// in a BYOP build.)
    pub fn load_conversation_by_server_token(
        &self,
        _server_token: &ServerConversationToken,
    ) -> warpui::r#async::BoxFuture<'static, Option<LoadedConversationData>> {
        box_future(futures::future::ready(None))
    }

    /// Loads a conversation from local DB and returns it.
    /// This is a private helper method. Use `get_load_conversation_data_future` instead.
    ///
    /// Note: This does NOT insert the conversation into memory. Callers are responsible
    /// for inserting the loaded conversation if needed.
    pub(super) fn load_conversation_from_db(
        &self,
        conversation_id: &AIConversationId,
    ) -> Option<AIConversation> {
        // First check if the conversation is in memory
        if let Some(conversation) = self.conversations_by_id.get(conversation_id) {
            return Some(conversation.clone());
        }

        // If not in memory, try to load from the database
        #[cfg(feature = "local_fs")]
        {
            let persisted_ai_conversation = self.db_connection.clone().and_then(|conn| {
                let mut conn = conn.lock().ok()?;

                let id_str = conversation_id.to_string();
                log::info!("Loading conversation {id_str} from db");
                match read_agent_conversation_by_id(&mut conn, &id_str) {
                    Ok(Some(conv)) => Some(conv),
                    Ok(None) => {
                        log::warn!("No AgentConversation found with id {id_str}");
                        None
                    }
                    Err(e) => {
                        log::warn!("Failed to read AgentConversation {id_str}: {e:?}");
                        None
                    }
                }
            });

            // Convert the persisted conversation to an AIConversation
            if let Some(persisted_conversation) = persisted_ai_conversation {
                if let Some(conversation) =
                    convert_persisted_conversation_to_ai_conversation(persisted_conversation)
                {
                    return Some(conversation);
                }
            }
        }

        None
    }

    /// Initializes historical conversations from restored agent conversations.
    ///
    /// At startup the conversations carry only `agent_conversations` records
    /// (empty task lists) whose summaries were computed at write time; tests
    /// may pass fully-hydrated conversations, whose summaries are derived
    /// from their tasks here. Preferring the persisted `summary` column
    /// means startup does not need to decode `agent_tasks` (or the tasks may
    /// not even be present) just to build the history list.
    pub(super) fn initialize_historical_conversations(
        &mut self,
        conversations: &[AgentConversation],
    ) {
        let conversations = conversations
            .iter()
            .sorted_by_key(|c| c.conversation.last_modified_at)
            .rev();

        let collected: HashMap<AIConversationId, AIConversationMetadata> = conversations
            .take(MAX_HISTORICAL_CONVERSATIONS)
            .filter_map(|agent_conv| {
                // Try to convert the conversation ID
                let conversation_id = match AIConversationId::try_from(
                    agent_conv.conversation.conversation_id.clone(),
                ) {
                    Ok(id) => id,
                    Err(e) => {
                        log::warn!("Failed to convert conversation ID: {e:?}");
                        return None;
                    }
                };

                // Prefer the write-time summary from the `summary` column;
                // fall back to deriving from tasks for fully-hydrated inputs
                // (and for rows written before the column existed).
                let summary = agent_conv
                    .conversation
                    .summary
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<AgentConversationSummary>(json).ok())
                    .unwrap_or_else(|| {
                        AgentConversationSummary::from_tasks(agent_conv.tasks.iter())
                    });

                if !summary.is_restorable {
                    return None;
                }

                // Child agent conversations are managed by their parent's
                // status card and should not appear in navigation/history.
                // Record the parent→child mapping before filtering so that
                // create_missing_child_agent_panes can discover children
                // before they are loaded into conversations_by_id.
                let conversation_data = serde_json::from_str::<AgentConversationData>(
                    &agent_conv.conversation.conversation_data,
                )
                .ok();
                if let Some(parent_id_str) = conversation_data
                    .as_ref()
                    .and_then(|data| data.parent_conversation_id.as_deref())
                {
                    if let Ok(parent_id) = AIConversationId::try_from(parent_id_str.to_string()) {
                        let children = self.children_by_parent.entry(parent_id).or_default();
                        if !children.contains(&conversation_id) {
                            children.push(conversation_id);
                        }
                    }

                    // Eagerly hydrate the child conversation -- and the
                    // agent-id/token indices `conversation_id_for_agent_id`
                    // reads -- into memory so orchestration transcript and
                    // pill-bar name resolution (`resolve_orchestration_participant`)
                    // can find a restored child before its parent's hidden
                    // pane materializes it lazily via `restore_conversations`.
                    // Restricted to orchestration children only; other
                    // historical conversations still load lazily. A
                    // subsequent `restore_conversations` call replaces this
                    // entry idempotently.
                    if let Some(data) = conversation_data.as_ref() {
                        if let Some(run_id) = data.run_id.as_deref() {
                            self.agent_id_to_conversation_id
                                .insert(run_id.to_owned(), conversation_id);
                        }
                        if let Some(token) = data.server_conversation_token.as_ref() {
                            self.server_token_to_conversation_id
                                .insert(ServerConversationToken::new(token.clone()), conversation_id);
                        }
                    }
                    let child_conversation = if agent_conv.tasks.is_empty() {
                        self.load_conversation_from_db(&conversation_id)
                    } else {
                        convert_persisted_conversation_to_ai_conversation_with_metadata(
                            agent_conv.clone(),
                        )
                    };
                    if let Some(child_conversation) = child_conversation {
                        self.conversations_by_id
                            .insert(conversation_id, child_conversation);
                    } else {
                        log::warn!(
                            "Failed to eagerly hydrate orchestration child {conversation_id}; \
                             pill bar / name resolution will fall back to lazy materialization",
                        );
                    }
                    return None;
                }

                // Skip conversations that only contain passive AutoCodeDiff
                // system queries the user never interacted with (past
                // accepting or rejecting the diff).
                if summary.is_unlisted_auto_code_diff {
                    return None;
                }

                let AgentConversationSummary {
                    initial_query,
                    title,
                    initial_working_directory,
                    ..
                } = summary;

                if initial_query.is_empty() {
                    log::debug!(
                        "Skipping legacy conversation {conversation_id} (no initial query)"
                    );
                    return None;
                }

                let credits_spent = conversation_data
                    .as_ref()
                    .and_then(|data| data.conversation_usage_metadata.as_ref())
                    .map(|m| m.credits_spent);
                let artifacts = conversation_data
                    .as_ref()
                    .and_then(|data| data.artifacts_json.as_ref())
                    .and_then(|json| serde_json::from_str(json).ok())
                    .unwrap_or_default();
                let parent_conversation_id = conversation_data
                    .as_ref()
                    .and_then(|data| data.parent_conversation_id.as_deref())
                    .and_then(|s| AIConversationId::try_from(s.to_string()).ok());
                let parent_agent_id = conversation_data
                    .as_ref()
                    .and_then(|data| data.parent_agent_id.clone());
                let server_conversation_token = conversation_data
                    .and_then(|data| data.server_conversation_token)
                    .map(ServerConversationToken::new);

                Some((conversation_id, AIConversationMetadata {
                    id: conversation_id,
                    title,
                    initial_query,
                    last_modified_at: agent_conv.conversation.last_modified_at,
                    initial_working_directory,
                    credits_spent,
                    server_conversation_token,
                    has_local_data: true,
                    artifacts,
                    ambient_agent_task_id: None,
                    parent_conversation_id,
                    parent_agent_id,
                }))
            })
            .collect();

        // Populate the token → conversation reverse index alongside the
        // forward metadata map.
        for (conversation_id, metadata) in &collected {
            if let Some(token) = &metadata.server_conversation_token {
                self.server_token_to_conversation_id
                    .insert(token.clone(), *conversation_id);
            }
        }
        self.all_conversations_metadata = collected;
    }
}
