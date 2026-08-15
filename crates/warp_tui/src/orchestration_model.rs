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
    StartAgentExecutionMode, StartAgentExecutor, StartAgentRequest, aggregated_orchestrator_status,
    child_conversations_in_pill_order, descendant_conversation_ids_in_spawn_order,
    descendant_conversations_in_pill_order, loaded_subtree_rollup,
    orchestration_root_conversation_id,
};
use warp_core::features::FeatureFlag;
use warpui::SingletonEntity;
use warpui_core::{AppContext, Entity, EntityId, ModelContext, ModelHandle};

use crate::orchestration_tab_bar::{
    TuiOrchestrationBreadcrumb, TuiOrchestrationChild, TuiOrchestrationSnapshot,
};
use crate::session_registry::{TuiSessionId, TuiSessions};
use crate::tab_bar::{TuiTabBarNavigationDirection, TuiTabBarPagingState};

/// The main-tab label used for the orchestration tree root.
pub(crate) const ORCHESTRATOR_TAB_LABEL: &str = "orchestrator";

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
    /// Paging intent shared by the per-session tab-bar views, tracked per
    /// rendered level (keyed by the level's anchor conversation) so paging
    /// within a drilled-in level does not disturb any other level's page.
    tab_bar_paging_by_anchor: HashMap<AIConversationId, TuiTabBarPagingState<AIConversationId>>,
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
            tab_bar_paging_by_anchor: HashMap::new(),
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
        // Drop explicit page state for levels whose anchor no longer exists.
        let history = BlocklistAIHistoryModel::as_ref(ctx);
        self.tab_bar_paging_by_anchor
            .retain(|anchor, _| history.conversation(anchor).is_some());
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

        let multi_level = FeatureFlag::MultiLevelOrchestration.is_enabled();
        let anchor_conversation_id = if multi_level {
            drill_down_anchor_id(
                history,
                &session_ids_by_conversation,
                selected_conversation_id,
                root_conversation_id,
            )
        } else {
            root_conversation_id
        };

        // One level (the anchor's DIRECT children) while multi-level is
        // enabled; the historical flat all-descendants projection otherwise.
        let ordered_children = if multi_level {
            child_conversations_in_pill_order(history, anchor_conversation_id)
        } else {
            descendant_conversations_in_pill_order(history, root_conversation_id)
        };
        let children = ordered_children
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
                    subtree_rollup: multi_level
                        .then(|| loaded_subtree_rollup(history, conversation_id))
                        .flatten(),
                })
            })
            .collect::<Vec<_>>();
        if children.is_empty() {
            return None;
        }

        let anchor_label = if anchor_conversation_id == root_conversation_id {
            ORCHESTRATOR_TAB_LABEL.to_owned()
        } else {
            conversation_agent_label(history, anchor_conversation_id)
        };
        let anchor_status =
            multi_level.then(|| aggregated_orchestrator_status(history, anchor_conversation_id));
        let anchor_navigable = session_ids_by_conversation.contains_key(&anchor_conversation_id);
        let breadcrumbs = if multi_level {
            breadcrumbs_for_anchor(
                history,
                &session_ids_by_conversation,
                anchor_conversation_id,
                root_conversation_id,
            )
        } else {
            Vec::new()
        };

        let resolved_page = self
            .tab_bar_paging_by_anchor
            .get(&anchor_conversation_id)
            .cloned()
            .unwrap_or_default()
            .resolve(children.first().map(|child| child.conversation_id), {
                let children = &children;
                move |anchor| {
                    children
                        .iter()
                        .any(|child| child.conversation_id == *anchor)
                }
            });
        Some(TuiOrchestrationSnapshot {
            root_conversation_id,
            anchor_conversation_id,
            anchor_label,
            anchor_status,
            anchor_navigable,
            breadcrumbs,
            selected_conversation_id,
            children,
            page_anchor: resolved_page.page_anchor,
            reveal_selected: resolved_page.reveal_selected,
        })
    }

    /// Stores an explicitly selected secondary page for one rendered level
    /// without switching sessions.
    pub(crate) fn set_explicit_page(
        &mut self,
        level_anchor: AIConversationId,
        page_anchor: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.tab_bar_paging_by_anchor
            .entry(level_anchor)
            .or_default()
            .set_explicit_anchor(page_anchor);
        ctx.notify();
    }

    /// Resolves the adjacent conversation in the whole orchestration tree:
    /// the root followed by every navigable descendant in pill order,
    /// wrapping at either end. This is the GUI's keyboard-cycling order
    /// (`adjacent_orchestration_child_conversation_id`) restricted to
    /// conversations with retained TUI sessions.
    pub(crate) fn adjacent_tree_conversation(
        &self,
        selected_conversation_id: AIConversationId,
        direction: TuiTabBarNavigationDirection,
        ctx: &AppContext,
    ) -> Option<AIConversationId> {
        let history = BlocklistAIHistoryModel::as_ref(ctx);
        let root_conversation_id =
            orchestration_root_conversation_id(history, selected_conversation_id)?;
        let session_ids_by_conversation =
            TuiSessions::as_ref(ctx).session_ids_by_conversation(history);
        let order = std::iter::once(root_conversation_id)
            .chain(
                descendant_conversations_in_pill_order(history, root_conversation_id)
                    .into_iter()
                    .map(|descendant| descendant.conversation_id),
            )
            .filter(|conversation_id| session_ids_by_conversation.contains_key(conversation_id))
            .collect::<Vec<_>>();
        let selected_index = order
            .iter()
            .position(|conversation_id| *conversation_id == selected_conversation_id)?;
        let target_index = match direction {
            TuiTabBarNavigationDirection::Previous => {
                selected_index.checked_sub(1).unwrap_or(order.len() - 1)
            }
            TuiTabBarNavigationDirection::Next => (selected_index + 1) % order.len(),
        };
        order.get(target_index).copied()
    }

    /// Focuses the retained session for a conversation and resumes automatic
    /// reveal on the level the bar will anchor to for that conversation.
    pub(crate) fn focus_conversation_session(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> Option<TuiSessionId> {
        let (session_id, level_anchor) = {
            let history = BlocklistAIHistoryModel::as_ref(ctx);
            let root_conversation_id =
                orchestration_root_conversation_id(history, conversation_id)?;
            let session_ids_by_conversation =
                TuiSessions::as_ref(ctx).session_ids_by_conversation(history);
            let session_id = *session_ids_by_conversation.get(&conversation_id)?;
            let level_anchor = if FeatureFlag::MultiLevelOrchestration.is_enabled() {
                drill_down_anchor_id(
                    history,
                    &session_ids_by_conversation,
                    conversation_id,
                    root_conversation_id,
                )
            } else {
                root_conversation_id
            };
            (session_id, level_anchor)
        };
        self.tab_bar_paging_by_anchor.remove(&level_anchor);
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

    /// Kills a child agent together with its entire loaded subtree,
    /// deepest-first, so no descendant session is orphaned. With multi-level
    /// orchestration disabled this only kills the child itself — depth > 1
    /// cannot arise from new activity while the flag is off.
    pub(crate) fn kill_child_agent_subtree(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        if FeatureFlag::MultiLevelOrchestration.is_enabled() {
            self.kill_descendant_agents(conversation_id, ctx);
        }
        self.kill_child_agent(conversation_id, ctx);
    }
}

/// Resolves the level the bar renders for a selection, mirroring the GUI's
/// `drill_down_anchor_id`: a selection that has at least one navigable child
/// anchors its own level, otherwise the bar shows the selection's parent
/// level. Filtered by session navigability so the row can never dead-end on
/// a child with no session to switch to.
fn drill_down_anchor_id(
    history: &BlocklistAIHistoryModel,
    session_ids_by_conversation: &HashMap<AIConversationId, TuiSessionId>,
    selected_conversation_id: AIConversationId,
    root_conversation_id: AIConversationId,
) -> AIConversationId {
    let has_navigable_child = history
        .child_conversation_ids_of(&selected_conversation_id)
        .iter()
        .any(|child_id| {
            session_ids_by_conversation.contains_key(child_id)
                && history.conversation(child_id).is_some()
        });
    if has_navigable_child {
        return selected_conversation_id;
    }
    history
        .conversation(&selected_conversation_id)
        .and_then(|conversation| {
            history.resolved_parent_conversation_id_for_conversation(conversation)
        })
        .unwrap_or(root_conversation_id)
}

/// Resolves the breadcrumb chips shown while the bar is drilled below the
/// tree root, mirroring the GUI's `breadcrumb_ids` rule: one chip for the
/// root, plus one for the anchor's direct parent when that parent is a
/// distinct intermediate level. Never more than two chips at any depth.
/// Chips must be selectable (they switch sessions), so a loaded but
/// sessionless parent contributes no chip.
fn breadcrumbs_for_anchor(
    history: &BlocklistAIHistoryModel,
    session_ids_by_conversation: &HashMap<AIConversationId, TuiSessionId>,
    anchor_conversation_id: AIConversationId,
    root_conversation_id: AIConversationId,
) -> Vec<TuiOrchestrationBreadcrumb> {
    if anchor_conversation_id == root_conversation_id {
        return Vec::new();
    }
    let mut breadcrumbs = vec![TuiOrchestrationBreadcrumb {
        conversation_id: root_conversation_id,
        label: ORCHESTRATOR_TAB_LABEL.to_owned(),
    }];
    let parent_id = history
        .conversation(&anchor_conversation_id)
        .and_then(|anchor| history.resolved_parent_conversation_id_for_conversation(anchor))
        .filter(|parent_id| {
            *parent_id != root_conversation_id
                && *parent_id != anchor_conversation_id
                && session_ids_by_conversation.contains_key(parent_id)
        });
    if let Some(parent_id) = parent_id {
        breadcrumbs.push(TuiOrchestrationBreadcrumb {
            conversation_id: parent_id,
            label: conversation_agent_label(history, parent_id),
        });
    }
    breadcrumbs
}

/// A conversation's non-empty agent name, or the shared fallback label.
fn conversation_agent_label(
    history: &BlocklistAIHistoryModel,
    conversation_id: AIConversationId,
) -> String {
    history
        .conversation(&conversation_id)
        .and_then(|conversation| conversation.agent_name())
        .filter(|name| !name.is_empty())
        .unwrap_or("Agent")
        .to_owned()
}

#[cfg(test)]
#[path = "orchestration_model_tests.rs"]
mod tests;
