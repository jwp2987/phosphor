//! [`TuiOrchestrationModel`]: TUI orchestration navigation state -- local only.
//!
//! Ported from the pin's `orchestration_model.rs` (679 lines), trimmed to what
//! is genuinely local and reachable in this fork. Two things were cut, not
//! stubbed:
//!
//! - **`dispatch_create_agent` and everything that materializes a new child
//!   session** (`begin_local_oz_child_launch`, `begin_remote_child_launch`,
//!   `register_local_oz_child_session`, `register_remote_child_session`,
//!   `finish_remote_child_launch`, `handle_streamer_event`,
//!   `cleanup_failed_child`, `fail_child_request`, `register_event_consumer`,
//!   `handle_session_removed`, and the `TuiOrchestrationEvent` enum that
//!   carries their requests). The pin drives all of this from a shared
//!   `StartAgentExecutor` singleton (`StartAgentRequest`,
//!   `prepare_local_oz_child_launch`) that **does not exist anywhere in this
//!   fork, not even for the GUI** -- this fork's local child-harness launch
//!   (`app/src/pane_group/pane/local_harness_launch.rs`) is a different,
//!   `pub(super)`-private mechanism scoped to `PaneGroup`'s hidden panes, not
//!   a shared executor a TUI session registry could subscribe to. Its
//!   *remote* half additionally calls `ServerApiProvider`/`ai_client` --
//!   cloud-runner orchestration, out of scope pending #290. Building a TUI
//!   equivalent of child materialization is future work, not a mechanical
//!   trim of this file.
//! - `OrchestrationEventStreamer`/`handle_streamer_event`: exists in the pin
//!   only to reflect a *remote* run's server-pushed status back onto its
//!   local conversation. With no remote children, there is nothing to
//!   stream.
//!
//! What remains is exactly what the orchestration tab bar's 9 pinned tests
//! exercise: registering the singleton, noticing when the conversation
//! topology changes, and computing a read-only navigation snapshot. The
//! snapshot is fed by `app::ai::blocklist::orchestration_topology`'s
//! already-local-only helpers (re-exported via `warp::tui_export`) rather
//! than reimplementing traversal -- that module's own doc comment already
//! states there is no remote-worker execution path in this fork.
use warp::tui_export::{
    AIConversationId, BlocklistAIHistoryEvent, BlocklistAIHistoryModel,
    descendant_conversations_in_pill_order, orchestration_root_conversation_id,
};
use warpui::SingletonEntity;
use warpui_core::{AppContext, Entity, ModelContext, ModelHandle};

use crate::orchestration_tab_bar::{TuiOrchestrationChild, TuiOrchestrationSnapshot};
use crate::session_registry::{TuiSessionId, TuiSessions};
use crate::tab_bar::TuiTabBarPagingState;

/// The TUI's orchestration singleton. See the module doc above for what was
/// cut relative to the pin.
pub(crate) struct TuiOrchestrationModel {
    /// Paging intent shared by the per-session tab-bar views.
    tab_bar_paging: TuiTabBarPagingState<AIConversationId>,
}

impl Entity for TuiOrchestrationModel {
    type Event = ();
}

impl SingletonEntity for TuiOrchestrationModel {}

impl TuiOrchestrationModel {
    /// Registers the singleton and subscribes it to conversation-topology
    /// changes so views can be told to refresh their tab bar.
    pub(crate) fn register(ctx: &mut AppContext) -> ModelHandle<Self> {
        let history = BlocklistAIHistoryModel::handle(ctx);
        let model = ctx.add_singleton_model(|_| Self {
            tab_bar_paging: TuiTabBarPagingState::default(),
        });
        let model_for_history = model.clone();
        ctx.subscribe_to_model(&history, move |_, event, ctx| {
            if Self::is_topology_change(event) {
                model_for_history.update(ctx, |model, ctx| model.topology_changed(ctx));
            }
        });
        model
    }

    /// Whether a history event can change the shape of an orchestration tree
    /// (who the children are, their order, or their status) rather than just
    /// their transcript content or per-view selection state.
    fn is_topology_change(event: &BlocklistAIHistoryEvent) -> bool {
        match event {
            BlocklistAIHistoryEvent::StartedNewConversation { .. }
            | BlocklistAIHistoryEvent::AppendedExchange { .. }
            | BlocklistAIHistoryEvent::UpdatedConversationStatus { .. }
            | BlocklistAIHistoryEvent::ClearedConversationsInTerminalView { .. }
            | BlocklistAIHistoryEvent::SplitConversation { .. }
            | BlocklistAIHistoryEvent::RemoveConversation { .. }
            | BlocklistAIHistoryEvent::DeletedConversation { .. }
            | BlocklistAIHistoryEvent::RestoredConversations { .. }
            | BlocklistAIHistoryEvent::UpdatedConversationMetadata { .. }
            | BlocklistAIHistoryEvent::ConversationTransferredBetweenTerminalViews { .. } => true,
            BlocklistAIHistoryEvent::CreatedSubtask { .. }
            | BlocklistAIHistoryEvent::UpgradedTask { .. }
            | BlocklistAIHistoryEvent::ReassignedExchange { .. }
            | BlocklistAIHistoryEvent::UpdatedStreamingExchange { .. }
            | BlocklistAIHistoryEvent::SetActiveConversation { .. }
            | BlocklistAIHistoryEvent::ClearedActiveConversation { .. }
            | BlocklistAIHistoryEvent::UpdatedTodoList { .. }
            | BlocklistAIHistoryEvent::UpdatedAutoexecuteOverride { .. }
            | BlocklistAIHistoryEvent::UpdatedConversationTitle { .. }
            | BlocklistAIHistoryEvent::UpdatedConversationArtifacts { .. }
            | BlocklistAIHistoryEvent::ConversationAgentIdAssigned { .. } => false,
        }
    }

    fn topology_changed(&mut self, ctx: &mut ModelContext<Self>) {
        ctx.notify();
    }

    /// Builds the current navigable tab tree for a selected conversation.
    pub(crate) fn snapshot(
        &self,
        selected_conversation_id: AIConversationId,
        ctx: &AppContext,
    ) -> Option<TuiOrchestrationSnapshot> {
        let history = BlocklistAIHistoryModel::as_ref(ctx);
        let root_conversation_id =
            orchestration_root_conversation_id(history, selected_conversation_id)?;
        let sessions = TuiSessions::as_ref(ctx);
        let session_ids_by_conversation = sessions.session_ids_by_conversation(history);
        session_ids_by_conversation.get(&root_conversation_id)?;

        let children = descendant_conversations_in_pill_order(history, root_conversation_id)
            .into_iter()
            .filter_map(|descendant| {
                let conversation_id = descendant.conversation_id;
                session_ids_by_conversation.get(&conversation_id)?;
                let conversation = history.conversation(&conversation_id)?;
                Some(TuiOrchestrationChild {
                    conversation_id,
                    label: conversation
                        .agent_name()
                        .filter(|name| !name.is_empty())
                        .unwrap_or("Agent")
                        .to_owned(),
                    spawn_index: descendant.spawn_index,
                    status: conversation.status().clone(),
                })
            })
            .collect::<Vec<_>>();
        if children.is_empty() {
            return None;
        }

        let resolved_page = self.tab_bar_paging.resolve(
            children.first().map(|child| child.conversation_id),
            |anchor| {
                children
                    .iter()
                    .any(|child| child.conversation_id == *anchor)
            },
        );
        Some(TuiOrchestrationSnapshot {
            root_conversation_id,
            selected_conversation_id,
            children,
            page_anchor: resolved_page.page_anchor,
            reveal_selected: resolved_page.reveal_selected,
        })
    }

    /// Stores an explicitly selected secondary page without switching sessions.
    pub(crate) fn set_explicit_page(
        &mut self,
        page_anchor: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.tab_bar_paging.set_explicit_anchor(page_anchor);
        ctx.notify();
    }

    /// Focuses the retained session for a conversation and resumes automatic reveal.
    pub(crate) fn focus_conversation_session(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> Option<TuiSessionId> {
        let history = BlocklistAIHistoryModel::as_ref(ctx);
        orchestration_root_conversation_id(history, conversation_id)?;
        let session_id = *TuiSessions::as_ref(ctx)
            .session_ids_by_conversation(history)
            .get(&conversation_id)?;
        self.tab_bar_paging.clear_explicit_anchor();
        TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
            sessions.focus_session(session_id, ctx);
        });
        Some(session_id)
    }
}
