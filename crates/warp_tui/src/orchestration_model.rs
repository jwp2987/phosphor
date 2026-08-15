//! [`TuiOrchestrationModel`]: TUI orchestration navigation state -- local only.
//!
//! Ported from the pin's `orchestration_model.rs` (679 lines), trimmed to what
//! is genuinely local and reachable in this fork.
//!
//! **Update, this port:** the shared `StartAgentExecutor` now exists
//! (`app/src/ai/blocklist/action_model/execute/start_agent.rs`, re-exported
//! via `warp::tui_export`) -- the claim below that it "does not exist
//! anywhere in this fork" is stale. [`dispatch_create_agent`],
//! [`fail_child_request`] and [`cleanup_child`] are built and wired to
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
//!
//!   **Scoped to the executor-dispatched path only.** Child agents *do*
//!   materialize in this fork by the user-invoked route: `/orchestrate`
//!   drives [`crate::pane_group::TuiPaneGroup::spawn_local_child_agents`],
//!   which creates a real hidden PTY-backed session per child and registers
//!   its conversation into the topology. The snapshot below, the tab bar it
//!   feeds, and the kill paths are therefore live code against real
//!   children, not staged-but-unreachable scaffolding.
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

#![allow(dead_code)]
// Staged port: this module came across from the pinned oracle (see `8c6d3a4c
// feat(tui): stage warp_tui crate ... (phase 0)` and the `port(tui)` commits) with
// the upstream API surface intact, but only the paths the TUI actually drives are
// wired up yet. The unused items here are upstream's, not ours.
//
// Kept rather than pruned because this fork re-pins against upstream roughly
// weekly (`ORACLE.md`); deleting upstream's helpers would turn each one into a
// re-pin conflict for no gain. Drop this attribute once the module is fully wired
// and check what is genuinely dead then.

use std::collections::{HashMap, HashSet};

use warp::tui_export::{
    AIConversationId, BlocklistAIHistoryEvent, BlocklistAIHistoryModel, ConversationStatus,
    StartAgentExecutionMode, StartAgentExecutor, StartAgentRequest,
    descendant_conversation_ids_in_spawn_order, descendant_conversations_in_pill_order,
    orchestration_root_conversation_id,
};
use warpui::SingletonEntity;
use warpui_core::{AppContext, Entity, EntityId, ModelContext, ModelHandle};

use crate::orchestration_tab_bar::{TuiOrchestrationChild, TuiOrchestrationSnapshot};
use crate::session_registry::{TuiSessionId, TuiSessions};
use crate::tab_bar::TuiTabBarPagingState;

/// Session-lifecycle work the orchestration model cannot perform inline.
///
/// Both variants are deferred rather than executed in place because the
/// caller is frequently the very view being torn down: the ctrl-c kill path
/// runs inside `TuiTerminalSessionView`'s own update, so reaching back into
/// that same view (to cancel its conversation) or dropping its session would
/// re-enter a live borrow. `TuiSessions::wire_orchestration` drains these
/// once the originating update has completed.
///
/// The pin carries a third, cloud-only variant pair for remote child launch
/// (`BeginRemoteChildLaunch`/`FinishRemoteChildLaunch`); those are declined
/// here (`DECLINED.md` #290) and `is_remote_child` is permanently false.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiOrchestrationEvent {
    /// Cancel the child's in-flight work through its own session view, then
    /// finish the teardown that was interrupted to get here.
    KillLocalChildSession {
        session_id: TuiSessionId,
        conversation_id: AIConversationId,
    },
    /// Drop a retained child session from the registry.
    RemoveChildSession(TuiSessionId),
}

/// The TUI's orchestration singleton. See the module doc above for what was
/// cut relative to the pin.
pub(crate) struct TuiOrchestrationModel {
    /// Paging intent shared by the per-session tab-bar views.
    tab_bar_paging: TuiTabBarPagingState<AIConversationId>,
    /// Session ids subscribed to a conversation's agent-event stream, keyed
    /// by the session consuming them. Always empty in this port: agent-event
    /// streams are a server-push concept and there is no streamer here
    /// (`DECLINED.md` #290), so nothing ever calls `register_event_consumer`.
    /// Kept so [`cleanup_child`]'s contract matches the pin's, and so a
    /// failed dispatch can be asserted not to have registered one.
    ///
    /// [`cleanup_child`]: Self::cleanup_child
    event_consumers_by_session: HashMap<TuiSessionId, HashSet<AIConversationId>>,
}

impl Entity for TuiOrchestrationModel {
    type Event = TuiOrchestrationEvent;
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

    /// Deletes a child conversation and drops the retained TUI session that
    /// was running it. Used both by the executor's `CleanupFailedChildLaunch`
    /// (a child that never got off the ground) and by [`Self::kill_child_agent`].
    ///
    /// Adapted from the pin, which reads the session out of its own
    /// `child_session_by_conversation` map. This fork has no such map on the
    /// model: `/orchestrate` children are materialized by
    /// [`crate::pane_group::TuiPaneGroup`], and the authoritative index from a
    /// conversation to the session hosting it is
    /// [`TuiSessions::session_ids_by_conversation`] -- the same index
    /// [`Self::snapshot`] navigates by, so a killable tab and a resolvable
    /// session are the same set by construction. It is read **before** the
    /// delete: the index is derived from live history, and `TuiPaneGroup`
    /// separately drops its own tracking entry in response to
    /// `DeletedConversation`, so a lookup afterwards would race both and leak
    /// the hidden PTY session.
    pub(crate) fn cleanup_child(
        &mut self,
        conversation_id: &AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let history = BlocklistAIHistoryModel::as_ref(ctx);
        let child_session_id = TuiSessions::as_ref(ctx)
            .session_ids_by_conversation(history)
            .get(conversation_id)
            .copied();
        let terminal_view_id = history.terminal_view_id_for_conversation(conversation_id);
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            history.delete_conversation(*conversation_id, terminal_view_id, ctx);
        });
        if let Some(session_id) = child_session_id {
            ctx.emit(TuiOrchestrationEvent::RemoveChildSession(session_id));
        }
        ctx.notify();
    }

    /// Kills a child agent: cancels any in-flight execution, deletes the
    /// conversation from history, and drops the retained TUI session.
    /// Equivalent to the GUI's `KillAgentConversation` path.
    ///
    /// Two of the pin's three steps are dropped as cloud, not overlooked:
    ///
    /// - The pin first tombstones the conversation via
    ///   `OrchestrationEventStreamer::mark_conversation_killed` so late SSE
    ///   events cannot resurrect a killed child. There is no event streamer
    ///   here (`DECLINED.md` #290) and therefore no late server events to
    ///   tombstone against -- a child's state only ever changes from inside
    ///   this process.
    /// - The pin then branches on `is_remote_child` to best-effort cancel the
    ///   server-side task through `ServerApiProvider`/`cancel_ambient_agent_task`.
    ///   `is_remote_child` is permanently false in this fork, so only the
    ///   local arm survives.
    ///
    /// **Divergence worth knowing:** the pin's local child is an in-app Oz
    /// conversation, so cancelling its controller *is* the kill. This fork's
    /// `/orchestrate` children are PTY-backed `claude` CLI processes
    /// ([`crate::pane_group::TuiPaneGroup::spawn_local_child_agents`]), so the
    /// controller cancel is usually a no-op and the actual kill is dropping
    /// the session, which drops its terminal manager and the child process
    /// with it. The cancel is kept regardless: children registered without a
    /// harness do run in-app, and the cancel path additionally tears down any
    /// computer-use background session for the conversation (see
    /// `BlocklistAIController::cancel_conversation_progress`).
    pub(crate) fn kill_child_agent(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        // Cancel in-flight execution BEFORE deletion, while the conversation
        // is still resolvable.
        let is_in_progress = BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(&conversation_id)
            .is_some_and(|conversation| {
                let status = conversation.status();
                status.is_in_progress() || status.is_blocked()
            });
        if is_in_progress {
            let history = BlocklistAIHistoryModel::as_ref(ctx);
            let child_session_id = TuiSessions::as_ref(ctx)
                .session_ids_by_conversation(history)
                .get(&conversation_id)
                .copied();
            if let Some(session_id) = child_session_id {
                // Cancelling reaches into the child's own view, which may be
                // the view currently driving this update. Defer it; the
                // handler resumes the teardown below while the conversation
                // is still available.
                ctx.emit(TuiOrchestrationEvent::KillLocalChildSession {
                    session_id,
                    conversation_id,
                });
                return;
            }
        }

        self.cleanup_child(&conversation_id, ctx);
    }

    /// Kills every descendant spawned by `conversation_id`, including nested
    /// descendants. Children are removed deepest-first so each retained
    /// session can tear down while its ancestry is still available.
    pub(crate) fn kill_descendant_agents(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let descendant_ids = descendant_conversation_ids_in_spawn_order(
            BlocklistAIHistoryModel::as_ref(ctx),
            conversation_id,
        );
        for descendant_id in descendant_ids.into_iter().rev() {
            self.kill_child_agent(descendant_id, ctx);
        }
    }
}

#[cfg(test)]
#[path = "orchestration_model_tests.rs"]
mod tests;
