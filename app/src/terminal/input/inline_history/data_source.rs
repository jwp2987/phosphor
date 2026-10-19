//! Data source for the inline history menu, providing both conversations and commands.
//!
//! Ordering semantics match the legacy up-arrow history menu:
//! - Items from different sessions appear before items from the current session
//! - Within each group, items are sorted by timestamp (oldest first)
//! - Commands are deduplicated, keeping the most recent occurrence
//! - The result is that current session items appear at the bottom (closer to input)

use crate::ai::agent::conversation::{AIConversationId, ConversationStatus};
use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::ai::blocklist::agent_view::AgentViewController;
use crate::input_suggestions::{HistoryInputSuggestion, HistoryOrder};
use crate::search::data_source::{Query, QueryFilter, QueryResult};
use crate::search::mixer::{AsyncDataSource, BoxFuture, DataSourceRunErrorWrapper};
use crate::terminal::history::{History, LinkedWorkflowData};
use crate::terminal::history::{UpArrowHistoryConfig, UpArrowSnapshot};
use crate::terminal::input::inline_history::search_item::InlineHistoryItem;
use crate::terminal::input::inline_menu::{
    InlineMenuAction, InlineMenuClickBehavior, InlineMenuType,
};
use crate::terminal::model::session::SessionId;
use crate::terminal::model::session::active_session::ActiveSession;
use chrono::{DateTime, Local};
use futures::channel::oneshot;
use fuzzy_match::FuzzyMatchResult;
use ordered_float::OrderedFloat;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use warpui::r#async::executor::{Background, BackgroundTask};
use warpui::{AppContext, Entity, EntityId, ModelHandle, SingletonEntity};

/// The most history rows the menu materialises for display (#874). Suggestions are sorted
/// oldest first, so the newest are at the tail; older rows are dropped *before* their text is
/// copied into menu entries. A typed prefix is applied before this cap, so older matches stay
/// reachable by typing more of the command.
const MAX_MENU_HISTORY_ROWS: usize = 500;

/// Keeps only the newest `max` of `items` (which are ordered oldest first), moving rather than
/// cloning the survivors.
fn newest_tail<T>(items: Vec<T>, max: usize) -> impl Iterator<Item = T> {
    let skip = items.len().saturating_sub(max);
    items.into_iter().skip(skip)
}

/// Applies the typed `prefix` first, then keeps the newest `max` survivors, so an old match is
/// still reachable by typing its prefix (#874). An empty prefix keeps everything before the cap.
fn prefix_filtered_newest_tail<T>(
    items: Vec<T>,
    prefix: &str,
    max: usize,
    text: impl Fn(&T) -> &str,
) -> impl Iterator<Item = T> {
    let filtered: Vec<T> = if prefix.is_empty() {
        items
    } else {
        items
            .into_iter()
            .filter(|item| text(item).starts_with(prefix))
            .collect()
    };
    newest_tail(filtered, max)
}

#[derive(Clone, Debug)]
pub enum AcceptHistoryItem {
    Conversation {
        conversation_id: AIConversationId,
        title: String,
    },
    Command {
        command: String,
        linked_workflow_data: Option<LinkedWorkflowData>,
    },
    AIPrompt {
        query_text: String,
    },
}

impl AcceptHistoryItem {
    pub fn buffer_replacement_text(&self) -> Option<&String> {
        match self {
            AcceptHistoryItem::Command { command, .. } => Some(command),
            AcceptHistoryItem::AIPrompt { query_text } => Some(query_text),
            AcceptHistoryItem::Conversation { .. } => None,
        }
    }
}

impl InlineMenuAction for AcceptHistoryItem {
    const MENU_TYPE: InlineMenuType = InlineMenuType::InlineHistoryMenu;

    fn click_behavior(&self) -> InlineMenuClickBehavior {
        match self {
            AcceptHistoryItem::Conversation { .. } => InlineMenuClickBehavior::AcceptOnClick,
            AcceptHistoryItem::Command { .. } | AcceptHistoryItem::AIPrompt { .. } => {
                InlineMenuClickBehavior::SelectOnClick
            }
        }
    }
}

/// Data source that provides both live conversations for a terminal view and command history.
pub struct InlineHistoryMenuDataSource {
    terminal_view_id: EntityId,
    active_session: ModelHandle<ActiveSession>,
    agent_view_controller: ModelHandle<AgentViewController>,
}

impl InlineHistoryMenuDataSource {
    pub fn new(
        terminal_view_id: EntityId,
        active_session: ModelHandle<ActiveSession>,
        agent_view_controller: ModelHandle<AgentViewController>,
    ) -> Self {
        Self {
            terminal_view_id,
            active_session,
            agent_view_controller,
        }
    }

    fn build_conversation_entries(&self, trimmed_query: &str, app: &AppContext) -> Vec<MenuEntry> {
        let mut conversation_entries: Vec<MenuEntry> = Vec::new();
        let history_model = BlocklistAIHistoryModel::handle(app).as_ref(app);
        for conversation in
            history_model.all_live_conversations_for_terminal_view(self.terminal_view_id)
        {
            if conversation.is_entirely_passive() || conversation.exchange_count() == 0 {
                continue;
            }

            let Some(timestamp) = conversation.last_modified_at() else {
                continue;
            };
            let title = conversation
                .title()
                .unwrap_or_else(|| crate::t!("conversation-untitled"));
            let match_result = if trimmed_query.is_empty() {
                None
            } else {
                let result = fuzzy_match::match_indices_case_insensitive(&title, trimmed_query);
                if result.is_none() || result.as_ref().is_some_and(|r| r.score < 50) {
                    continue;
                }
                result
            };

            conversation_entries.push(MenuEntry {
                order: HistoryOrder::CurrentSession,
                sort_timestamp: timestamp,
                item: MenuItem::Conversation {
                    conversation_id: conversation.id(),
                    title,
                    status: conversation.status().clone(),
                    match_result,
                    display_timestamp: timestamp,
                },
            });
        }

        conversation_entries.sort_by(|a, b| {
            a.order
                .cmp(&b.order)
                .then(a.sort_timestamp.cmp(&b.sort_timestamp))
        });
        conversation_entries
    }
}

#[derive(Clone)]
struct MenuEntry {
    order: HistoryOrder,
    sort_timestamp: DateTime<Local>,
    item: MenuItem,
}

#[derive(Clone)]
enum MenuItem {
    Conversation {
        conversation_id: AIConversationId,
        title: String,
        status: ConversationStatus,
        match_result: Option<FuzzyMatchResult>,
        display_timestamp: DateTime<Local>,
    },
    Command {
        command: String,
        linked_workflow_data: Option<LinkedWorkflowData>,
        display_timestamp: DateTime<Local>,
        prefix_match_len: usize,
    },
}

fn interleave_conversations(base: Vec<MenuEntry>, conversations: Vec<MenuEntry>) -> Vec<MenuEntry> {
    let current_start_idx = base
        .iter()
        .position(|e| e.order == HistoryOrder::CurrentSession)
        .unwrap_or(base.len());

    // Move entries rather than cloning them: with a huge history each clone copies the
    // command text (#874).
    let mut merged: Vec<MenuEntry> = Vec::with_capacity(base.len() + conversations.len());
    let mut base = base.into_iter();
    merged.extend(base.by_ref().take(current_start_idx));

    let mut base_current = base.peekable();
    let mut conversations = conversations;
    conversations.sort_by(|a, b| a.sort_timestamp.cmp(&b.sort_timestamp));

    for conv in conversations {
        while let Some(entry) = base_current.next_if(|e| e.sort_timestamp < conv.sort_timestamp) {
            merged.push(entry);
        }
        merged.push(conv);
    }
    merged.extend(base_current);

    merged
}

/// Everything a history menu query needs, captured on the main thread so the expensive part
/// (filter, sort, dedupe, row construction) can run on a background thread (#874).
pub(crate) struct MenuQuerySnapshot {
    is_agent_view: bool,
    include_commands: bool,
    up_arrow: UpArrowSnapshot,
    trimmed_query: String,
    session_id: Option<SessionId>,
    /// Live conversations matching the query. Few, so built on the main thread.
    conversations: Vec<MenuEntry>,
}

impl MenuQuerySnapshot {
    /// Builds the menu rows. Pure: touches no app state, so it is safe off the main thread.
    pub(crate) fn build_results(self) -> Vec<QueryResult<AcceptHistoryItem>> {
        self.build_results_cancellable(&|| false)
            .unwrap_or_default()
    }

    /// [`Self::build_results`], giving up (`None`) once `cancelled` turns true. Checked between
    /// the filter, sort and dedupe phases, the O(N log N) part.
    pub(crate) fn build_results_cancellable(
        self,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Vec<QueryResult<AcceptHistoryItem>>> {
        let prefix_match_len = self.trimmed_query.len();
        if self.is_agent_view {
            return self.build_agent_view_results(prefix_match_len, cancelled);
        }
        let Self {
            include_commands,
            up_arrow,
            trimmed_query,
            session_id,
            conversations,
            ..
        } = self;

        let command_entries = if include_commands {
            let suggestions = up_arrow.suggestions_cancellable(
                (!trimmed_query.is_empty()).then_some(trimmed_query.as_str()),
                cancelled,
            )?;
            newest_tail(suggestions, MAX_MENU_HISTORY_ROWS)
                .filter_map(|suggestion| {
                    let HistoryInputSuggestion::Command { entry } = &suggestion else {
                        return None;
                    };

                    let command = entry.command.trim();
                    if command.is_empty() {
                        return None;
                    }
                    if !trimmed_query.is_empty() && !command.starts_with(trimmed_query.as_str()) {
                        return None;
                    }

                    let order =
                        suggestion.history_order(session_id, up_arrow.all_live_session_ids());
                    let sort_timestamp = entry.start_ts.unwrap_or_default();
                    let display_timestamp = entry.start_ts.unwrap_or_else(Local::now);

                    Some(MenuEntry {
                        order,
                        sort_timestamp,
                        item: MenuItem::Command {
                            command: command.to_string(),
                            linked_workflow_data: entry.linked_workflow_data(),
                            display_timestamp,
                            prefix_match_len,
                        },
                    })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let merged_entries = interleave_conversations(command_entries, conversations);

        let mut results: Vec<QueryResult<AcceptHistoryItem>> = Vec::new();
        for entry in merged_entries {
            let score = OrderedFloat(results.len() as f64);
            let search_item = match entry.item {
                MenuItem::Conversation {
                    conversation_id,
                    title,
                    status,
                    match_result,
                    display_timestamp,
                } => InlineHistoryItem::conversation(
                    conversation_id,
                    title,
                    status,
                    display_timestamp,
                )
                .with_name_match_result(match_result),
                MenuItem::Command {
                    command,
                    linked_workflow_data,
                    display_timestamp,
                    prefix_match_len,
                } => InlineHistoryItem::command(command, linked_workflow_data, display_timestamp)
                    .with_prefix_match_len(prefix_match_len),
            };

            results.push(QueryResult::from(search_item.with_score(score)));
        }

        Some(results)
    }

    fn build_agent_view_results(
        self,
        prefix_match_len: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Vec<QueryResult<AcceptHistoryItem>>> {
        let suggestions = self.up_arrow.suggestions_cancellable(None, cancelled)?;

        let mut results: Vec<QueryResult<AcceptHistoryItem>> = Vec::new();
        for suggestion in prefix_filtered_newest_tail(
            suggestions,
            &self.trimmed_query,
            MAX_MENU_HISTORY_ROWS,
            |suggestion| suggestion.text(),
        ) {
            let (search_item, score) = match suggestion {
                HistoryInputSuggestion::Command { entry } => {
                    let command = entry.command.trim();
                    if command.is_empty() {
                        continue;
                    }
                    let timestamp = entry.start_ts.unwrap_or_else(Local::now);
                    (
                        InlineHistoryItem::command(
                            command.to_string(),
                            entry.linked_workflow_data(),
                            timestamp,
                        )
                        .with_prefix_match_len(prefix_match_len),
                        OrderedFloat(results.len() as f64),
                    )
                }
                HistoryInputSuggestion::AIQuery { entry } => {
                    let query_text = entry.query_text.trim();
                    if query_text.is_empty() {
                        continue;
                    }
                    (
                        InlineHistoryItem::ai_prompt(query_text.to_string(), entry.start_time)
                            .with_prefix_match_len(prefix_match_len),
                        OrderedFloat(results.len() as f64),
                    )
                }
            };

            results.push(QueryResult::from(search_item.with_score(score)));
        }

        Some(results)
    }
}

impl InlineHistoryMenuDataSource {
    /// The main-thread half of a query: reads app state and clones `Arc`s, nothing more.
    fn snapshot(&self, query: &Query, app: &AppContext) -> MenuQuerySnapshot {
        let trimmed_query = query.text.trim();
        let session_id = self.active_session.as_ref(app).session(app).map(|s| s.id());
        let is_agent_view = self.agent_view_controller.as_ref(app).is_active();
        let history = History::handle(app).as_ref(app);

        let (include_commands, include_prompts, conversations) = if is_agent_view {
            (
                query.filters.is_empty() || query.filters.contains(&QueryFilter::Commands),
                query.filters.is_empty() || query.filters.contains(&QueryFilter::PromptHistory),
                Vec::new(),
            )
        } else {
            let include_conversations =
                query.filters.is_empty() || query.filters.contains(&QueryFilter::Conversations);
            (
                query.filters.is_empty() || query.filters.contains(&QueryFilter::Commands),
                false,
                if include_conversations {
                    self.build_conversation_entries(trimmed_query, app)
                } else {
                    Vec::new()
                },
            )
        };

        MenuQuerySnapshot {
            is_agent_view,
            include_commands,
            up_arrow: UpArrowSnapshot::capture(
                history,
                self.terminal_view_id,
                session_id,
                UpArrowHistoryConfig {
                    include_commands,
                    include_prompts,
                },
                app,
            ),
            trimmed_query: trimmed_query.to_string(),
            session_id,
            conversations,
        }
    }
}

/// Histories up to this many entries are built inline on the calling thread, which costs a
/// few milliseconds at most (about 1 ms per 5k entries in release), so a normal user keeps
/// the old synchronous behaviour: the rows exist the moment the query returns.
pub(crate) const INLINE_BUILD_MAX_ENTRIES: usize = 5_000;

impl AsyncDataSource for InlineHistoryMenuDataSource {
    type Action = AcceptHistoryItem;

    fn run_query_inline(
        &self,
        query: &Query,
        app: &AppContext,
    ) -> Option<Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper>> {
        let session_id = self.active_session.as_ref(app).session(app).map(|s| s.id());
        let bound = session_id.map_or(0, |id| {
            History::handle(app)
                .as_ref(app)
                .visible_commands_upper_bound(id)
        });
        (bound <= INLINE_BUILD_MAX_ENTRIES).then(|| Ok(self.snapshot(query, app).build_results()))
    }

    fn run_query(
        &self,
        query: &Query,
        app: &AppContext,
    ) -> BoxFuture<'static, Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper>> {
        let snapshot = self.snapshot(query, app);
        let background = app.background_executor().clone();
        Box::pin(async move { Ok(build_results_in_background(&background, snapshot).await) })
    }
}

/// When the awaiting future is dropped (a newer query superseded this one) this raises a flag
/// the background build checks between its filter, sort and dedupe phases, and aborts the task
/// if it has not started yet. A build already inside a sort runs that one phase to completion
/// (a sort cannot be interrupted) and then stops; its result is discarded.
struct CancelOnDrop {
    cancelled: Arc<AtomicBool>,
    task: BackgroundTask,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.task.abort();
    }
}

/// Runs [`MenuQuerySnapshot::build_results_cancellable`] on the background executor and awaits
/// the result, so the main thread only polls a oneshot.
pub(crate) async fn build_results_in_background(
    background: &Background,
    snapshot: MenuQuerySnapshot,
) -> Vec<QueryResult<AcceptHistoryItem>> {
    let (tx, rx) = oneshot::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let task = background.spawn(async move {
        if let Some(results) = snapshot.build_results_cancellable(&|| flag.load(Ordering::Relaxed))
        {
            let _ = tx.send(results);
        }
    });
    let _guard = CancelOnDrop { cancelled, task };
    // The sender is only dropped unsent if the task was cancelled or panicked; both mean there
    // is nothing to show for this query.
    rx.await.unwrap_or_default()
}

impl Entity for InlineHistoryMenuDataSource {
    type Event = ();
}

#[cfg(test)]
#[path = "data_source_tests.rs"]
mod tests;
