//! [`TuiOrchestrationModel`]: TUI orchestration navigation state -- local only.
//!
//! Ported from the pin's `orchestration_model.rs` (679 lines), trimmed to what
//! is genuinely local and reachable in this fork.
//!
//! **Update, this port:** the shared `StartAgentExecutor` now exists
//! (`app/src/ai/blocklist/action_model/execute/start_agent.rs`, re-exported
//! via `warp::tui_export`) -- the claim below that it "does not exist
//! anywhere in this fork" is stale. [`dispatch_create_agent`],
//! [`fail_child_request`] and [`cleanup_failed_child`] are built and wired to
//! it, so a TUI-dispatched
//! `StartAgentExecutionMode::Local` request now resolves as a clean failure
//! (matching the pin's own "not supported outside of dogfood" behaviour for
//! named-harness children) instead of never being handled at all. What is
//! **still** cut, not stubbed:
//!
//! - **Native local-Oz child materialization** (`begin_local_oz_child_launch`,
//!   `register_local_oz_child_session`, and the session-side half of
//!   `TuiOrchestrationEvent`/`register_event_consumer`/
//!   `handle_session_removed`). The pin's only implementation routes through
//!   `prepare_local_oz_child_launch`, which -- despite its name -- creates the
//!   child's task via `ServerApiProvider::as_ref(ctx).get_ai_client()
//!   .create_agent_task(..)`, i.e. it is cloud-coupled even in "local" mode.
//!   `start_agent.rs`'s module doc records this seam in detail. Building a
//!   genuinely local replacement (materializing a real TUI session the way
//!   `app/src/pane_group/pane/local_harness_launch.rs` already does for the
//!   GUI's hidden panes) is future work, not a mechanical trim of this file.
//! - **Everything remote**: `begin_remote_child_launch`,
//!   `register_remote_child_session`, `finish_remote_child_launch`,
//!   `OrchestrationEventStreamer`/`handle_streamer_event`. Declined
//!   cloud-runner orchestration (`DECLINED.md` #290); `is_remote_child` is
//!   permanently false here, so `StartAgentExecutionMode` in this fork has no
//!   `Remote` variant to route at all.
//!
//! What remains is exactly what the orchestration tab bar's pinned tests
//! exercise: registering the singleton, noticing when the conversation
//! topology changes, computing a read-only navigation snapshot, and (new in
//! this port) resolving an unsupported local-harness dispatch as a clean,
//! fully-cleaned-up failure. The snapshot is fed by
//! `app::ai::blocklist::orchestration_topology`'s already-local-only helpers
//! (re-exported via `warp::tui_export`) rather than reimplementing traversal
//! -- that module's own doc comment already states there is no remote-worker
//! execution path in this fork.
use std::collections::{HashMap, HashSet};

use warp::tui_export::{
    AIConversationId, BlocklistAIHistoryEvent, BlocklistAIHistoryModel, ConversationStatus,
    StartAgentExecutionMode, StartAgentExecutor, StartAgentRequest,
    descendant_conversations_in_pill_order, orchestration_root_conversation_id,
};
use warpui::SingletonEntity;
use warpui_core::{AppContext, Entity, EntityId, ModelContext, ModelHandle};

use crate::orchestration_tab_bar::{TuiOrchestrationChild, TuiOrchestrationSnapshot};
use crate::session_registry::{TuiSessionId, TuiSessions};
use crate::tab_bar::TuiTabBarPagingState;

/// The TUI's orchestration singleton. See the module doc above for what was
/// cut relative to the pin.
pub(crate) struct TuiOrchestrationModel {
    /// Paging intent shared by the per-session tab-bar views.
    tab_bar_paging: TuiTabBarPagingState<AIConversationId>,
    /// Session ids subscribed to a conversation's agent-event stream, keyed
    /// by the session consuming them. Always empty in this port: nothing
    /// yet materializes a session for a dispatched child (see the module
    /// doc), so nothing ever calls `register_event_consumer`. Kept so
    /// [`cleanup_failed_child`]'s contract matches the pin's, and so a
    /// failed dispatch can be asserted not to have registered one.
    ///
    /// [`cleanup_failed_child`]: Self::cleanup_failed_child
    event_consumers_by_session: HashMap<TuiSessionId, HashSet<AIConversationId>>,
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
            event_consumers_by_session: HashMap::new(),
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

    /// Routes a `CreateAgent` request from a [`StartAgentExecutor`] the
    /// caller dispatched through. `executor` is that same handle, passed
    /// through so a resolved outcome can be reported straight back to it --
    /// see the module doc and `start_agent.rs`'s doc comment for why this
    /// fork resolves directly instead of via a history-event broadcast.
    ///
    /// Every `Local` request currently resolves as a clean failure:
    /// named-harness children (`harness_type: Some(_)`) because the pin
    /// itself does not support them in the TUI ("Local non-oz children are
    /// not supported outside of dogfood in the GUI, and would be odd in the
    /// TUI"), and native local-Oz children (`harness_type: None`) because
    /// this fork has not yet built a non-cloud replacement for the pin's
    /// `prepare_local_oz_child_launch` (see the module doc). `parent_session_id`
    /// is unused until that replacement exists to hand a materialized
    /// session back to.
    pub(crate) fn dispatch_create_agent(
        &mut self,
        _parent_session_id: TuiSessionId,
        request: StartAgentRequest,
        executor: &ModelHandle<StartAgentExecutor>,
        ctx: &mut ModelContext<Self>,
    ) {
        let message = match &request.execution_mode {
            StartAgentExecutionMode::Local {
                harness_type: Some(harness_type),
                ..
            } => {
                format!("Local {harness_type} child agents aren't supported in Warp Agent CLI yet.")
            }
            StartAgentExecutionMode::Local {
                harness_type: None, ..
            } => "Native local child agents are not yet available in this fork.".to_string(),
        };
        self.fail_child_request(&request, message, executor, ctx);
    }

    /// Resolves a child request as failed without creating a TUI session.
    /// Ported from the pin, adapted to resolve the executor directly (see
    /// the module doc's seam note) instead of through a
    /// `NewConversationRequestComplete` history-event broadcast, which needs
    /// server-assigned conversation state this fork does not have.
    fn fail_child_request(
        &mut self,
        request: &StartAgentRequest,
        message: String,
        executor: &ModelHandle<StartAgentExecutor>,
        ctx: &mut ModelContext<Self>,
    ) {
        let request_id = request.id;
        log::warn!("Failing TUI child agent request: request_id={request_id:?}");
        let surface_id = EntityId::new();
        let conversation_id = BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            let conversation_id = history.start_new_child_conversation(
                surface_id,
                request.name.trim().to_owned(),
                request.parent_conversation_id,
                None,
                ctx,
            );
            history.update_conversation_status_with_error_message(
                surface_id,
                conversation_id,
                ConversationStatus::Error,
                Some(message.clone()),
                ctx,
            );
            conversation_id
        });
        executor.update(ctx, |executor, ctx| {
            executor.resolve_error(request_id, conversation_id, message, ctx);
        });
    }

    /// Tears down the ephemeral conversation of a child that failed at the
    /// launch stage (the executor's `CleanupFailedChildLaunch`). Nothing in
    /// this port materializes a session for a `Local` request (see the
    /// module doc), so unlike the pin there is no session-side half to tear
    /// down here yet -- only the failed-child conversation itself.
    pub(crate) fn cleanup_failed_child(
        &mut self,
        conversation_id: &AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let terminal_view_id =
            BlocklistAIHistoryModel::as_ref(ctx).terminal_view_id_for_conversation(conversation_id);
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            history.delete_conversation(*conversation_id, terminal_view_id, ctx);
        });
        ctx.notify();
    }
}

#[cfg(test)]
#[path = "orchestration_model_tests.rs"]
mod tests;
