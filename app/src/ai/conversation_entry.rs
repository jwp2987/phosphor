//! BYOP-local normalized conversation-list entries and the shared query ranking.
//!
//! Warp's upstream `agent_conversations_model` carries a much richer entry type wired to
//! ambient cloud runs, server conversation tokens, sessions, artifacts, and per-principal
//! usage. Zap is BYOP: conversations are local, there is no cloud run/session/token layer,
//! and no usage billing. This module keeps a minimal faithful projection — just the fields
//! the conversation-list surfaces (the TUI `/conversations` menu and the selection policy)
//! actually read — plus the recency/fuzzy ranking ported verbatim from upstream.

use chrono::{DateTime, Utc};
use fuzzy_match::{FuzzyMatchResult, match_indices_case_insensitive};
use warp_cli::agent::Harness;
use warpui::AppContext;

use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent_conversations_model::AgentRunDisplayStatus;
use crate::ai::ambient_agents::AmbientAgentTaskId;

pub(super) const DEFAULT_RESULT_COUNT: usize = 50;
pub(super) const MAX_SEARCH_RESULTS: usize = 500;
const MINIMUM_FUZZY_SCORE: i64 = 25;

/// Stable projection identity used by list and navigation surfaces.
///
/// The `AmbientRun` variant is retained for shape-parity with upstream navigation code;
/// BYOP-local surfaces only ever produce and consume `Conversation` entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentConversationEntryId {
    AmbientRun(AmbientAgentTaskId),
    Conversation(AIConversationId),
}

impl AgentConversationEntryId {
    pub fn as_key(&self) -> String {
        match self {
            AgentConversationEntryId::AmbientRun(id) => format!("task_{id}"),
            AgentConversationEntryId::Conversation(id) => format!("conv_{id}"),
        }
    }
}

/// Navigation request input for resolving an entry to a `WorkspaceAction` at action time.
///
/// Upstream also has a `ServerToken(ServerConversationToken)` variant, used as a
/// cloud-transcript-viewer fallback when there is no local entry for a raw server
/// token. BYOP has no such fallback path (`AgentConversationIdentity::server_conversation_token`
/// is always `None` here, see its doc comment), and upstream marks that variant
/// `#[allow(dead_code)]` itself, so it is not carried over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentConversationNavigationSubject {
    Entry(AgentConversationEntryId),
}

/// Cross-system identifiers that may refer to the same underlying conversation.
///
/// BYOP has no cloud layer, so `server_conversation_token` is always `None`; the field is
/// kept so the selection policy reads the same shape as upstream without special-casing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentConversationIdentity {
    pub local_conversation_id: Option<AIConversationId>,
    pub server_conversation_token: Option<crate::ai::agent::api::ServerConversationToken>,
}

/// Display-only fields for rendering a conversation entry without consulting source models.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentConversationDisplayData {
    pub title: String,
    pub last_updated: DateTime<Utc>,
    pub status: AgentRunDisplayStatus,
    pub harness: Option<Harness>,
}

/// Normalized row data for the conversation list and selection surfaces.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentConversationEntry {
    pub id: AgentConversationEntryId,
    pub identity: AgentConversationIdentity,
    pub display: AgentConversationDisplayData,
}

impl AgentConversationEntry {
    /// BYOP has no cloud/ambient agent runs; local conversation entries are never cloud runs.
    pub fn is_cloud_agent_run(&self) -> bool {
        false
    }
}

/// Frontend-specific classification of a normalized conversation-list entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentConversationListEntryState {
    Selected,
    OpenElsewhere,
    Available,
    Unavailable,
}

/// Per-frontend policy for classifying normalized conversation-list entries.
pub trait AgentConversationListPolicy: 'static {
    /// Classifies `entry` as selected, open elsewhere, available, or unavailable.
    fn classify_entry(
        &self,
        entry: &AgentConversationEntry,
        app: &AppContext,
    ) -> AgentConversationListEntryState;
}

/// A normalized conversation entry paired with optional title-match metadata.
pub struct AgentConversationQueryResult {
    pub entry: AgentConversationEntry,
    pub title_match: Option<FuzzyMatchResult>,
}

/// Applies the shared conversation-menu recency and fuzzy-ranking policy.
///
/// Ported verbatim from upstream `agent_conversations_model::query`: empty query returns the
/// most-recent `DEFAULT_RESULT_COUNT` entries oldest-first; a non-empty query fuzzy-matches
/// titles above `MINIMUM_FUZZY_SCORE`, ranked by (score, recency).
pub fn query_conversation_entries(
    mut entries: Vec<AgentConversationEntry>,
    query: &str,
) -> Vec<AgentConversationQueryResult> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        entries.sort_by(|a, b| b.display.last_updated.cmp(&a.display.last_updated));
        entries.truncate(DEFAULT_RESULT_COUNT);
        entries.reverse();
        return entries
            .into_iter()
            .map(|entry| AgentConversationQueryResult {
                entry,
                title_match: None,
            })
            .collect();
    }

    let mut matches = entries
        .into_iter()
        .filter_map(|entry| {
            let title_match = match_indices_case_insensitive(&entry.display.title, &query)?;
            (title_match.score >= MINIMUM_FUZZY_SCORE).then_some(AgentConversationQueryResult {
                entry,
                title_match: Some(title_match),
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|result| {
        let score = result
            .title_match
            .as_ref()
            .map_or(i64::MIN, |title_match| title_match.score);
        (score, result.entry.display.last_updated.timestamp_millis())
    });
    if matches.len() > MAX_SEARCH_RESULTS {
        matches.drain(..matches.len() - MAX_SEARCH_RESULTS);
    }
    matches
}
