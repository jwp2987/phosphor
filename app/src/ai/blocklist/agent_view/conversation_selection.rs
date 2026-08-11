//! GUI implementation of [`ConversationSelection`], backed unconditionally by Agent View.
//!
//! Ported from the pin (`app/src/ai/blocklist/agent_view/conversation_selection.rs`, `02b53fcd8`)
//! for #316. The TUI already has its own implementation
//! (`crates/warp_tui/src/conversation_selection.rs`, `TuiConversationSelection`); this fills the
//! matching gap on the GUI side, which until now had no `ConversationSelection` impl at all --
//! `agent_view::AgentViewController` was used directly instead (see #343, blocked on this).
//!
//! Adapted, not verbatim:
//! - Field/method names follow this fork's conventions: `terminal_view_id` (not
//!   `terminal_surface_id`), `BlocklistAIHistoryEvent::ClearedConversationsInTerminalView` (not
//!   `...ForTerminalSurface`), and `event.terminal_view_id()` (not `.terminal_surface_id()`).
//! - `classify_entry`'s "open elsewhere" check no longer goes through
//!   `ActiveAgentViewsModel::get_terminal_view_id_for_entry`, which is permanently removed here
//!   (see `DECLINED.md`). Its two lookups are reproduced directly against
//!   `BlocklistAIHistoryModel`: `terminal_view_id_for_ambient_task` (new, added for this issue)
//!   for `AgentConversationEntryId::AmbientRun` entries, and the pre-existing
//!   `terminal_view_id_for_conversation` for `AgentConversationEntryId::Conversation` entries.
//!   The pin instead branched on `entry.identity.ambient_agent_task_id`/`.local_conversation_id`;
//!   this fork's `AgentConversationIdentity` has no `ambient_agent_task_id` field (BYOP-local
//!   surfaces "only ever produce and consume `Conversation` entries" per
//!   `ai/conversation_entry.rs`'s module doc), so branching on `entry.id`'s variant directly
//!   carries the same information without adding an always-`None` field to a shared struct.
//! - `has_open_action` (pin: `agent_conversations_model/entry.rs:273`, via
//!   `AgentConversationsModel::resolve_open_action`) doesn't exist here -- that whole
//!   navigation-resolution subsystem is part of the richer cloud/ambient `agent_conversations_model`
//!   this fork's `ai/conversation_entry.rs` deliberately does not carry. `classify_gui_list_entry`
//!   substitutes the one piece of it that has a local meaning: a non-Oz-harness entry (a
//!   CLI-subagent conversation) is `Unavailable` for "enter Agent View", mirroring the TUI's
//!   `classify_conversation_list_entry`. Every entry `AgentConversationsModel::get_entries`
//!   currently emits is Oz-harness, so this rarely triggers today (see that fn's own doc comment)
//!   -- but it is not purely decorative, either: `AIConversation::orchestration_harness` already
//!   lets a local orchestration child carry a different harness. Previously this file's doc said
//!   availability was simply "not open elsewhere" and `Unavailable` was unreachable; that was
//!   wrong -- the variant existed but nothing ever threaded a predicate that could produce it.

use warp_cli::agent::Harness;
use warp_core::report_error;
use warpui::{AppContext, EntityId, ModelContext, ModelHandle, SingletonEntity};

use super::{
    AgentViewController, AgentViewControllerEvent, AgentViewEntryOrigin, EnterAgentViewError,
};
use crate::ai::agent::conversation::{AIConversationAutoexecuteMode, AIConversationId};
use crate::ai::blocklist::conversation_selection::{
    ConversationSelection, ConversationSelectionEvent,
};
use crate::ai::blocklist::{BlocklistAIHistoryEvent, BlocklistAIHistoryModel};
use crate::ai::conversation_entry::{
    AgentConversationEntry, AgentConversationEntryId, AgentConversationListEntryState,
    AgentConversationListPolicy,
};

/// GUI conversation selection backed unconditionally by Agent View.
pub(crate) struct AgentViewConversationSelection {
    terminal_view_id: EntityId,
    agent_view_controller: ModelHandle<AgentViewController>,
}

impl AgentViewConversationSelection {
    /// Creates GUI conversation selection for a terminal view.
    pub(crate) fn new(
        terminal_view_id: EntityId,
        agent_view_controller: ModelHandle<AgentViewController>,
        ctx: &mut ModelContext<Box<dyn ConversationSelection>>,
    ) -> Self {
        ctx.subscribe_to_model(&agent_view_controller, |_, event, ctx| match event {
            AgentViewControllerEvent::EnteredAgentView {
                display_mode,
                origin,
                ..
            } => {
                ctx.emit(ConversationSelectionEvent::Changed);
                ctx.emit(ConversationSelectionEvent::Activated {
                    is_fullscreen: display_mode.is_fullscreen(),
                    origin: origin.clone(),
                });
            }
            AgentViewControllerEvent::ExitedAgentView {
                conversation_id,
                final_exchange_count,
                is_exit_before_new_entrance,
                ..
            } => {
                ctx.emit(ConversationSelectionEvent::Changed);
                ctx.emit(ConversationSelectionEvent::Deactivated {
                    conversation_id: *conversation_id,
                    final_exchange_count: *final_exchange_count,
                    is_exit_before_new_entrance: *is_exit_before_new_entrance,
                });
            }
            AgentViewControllerEvent::ExitConfirmed { .. } => {}
        });
        ctx.subscribe_to_model(
            &BlocklistAIHistoryModel::handle(ctx),
            |selection, event, ctx| selection.handle_history_event(event, ctx),
        );
        Self {
            terminal_view_id,
            agent_view_controller,
        }
    }
}

/// Applies GUI list-state precedence without consulting frontend models.
///
/// `harness` is the entry's resolved execution harness. Agent View's "select
/// existing conversation" flow (`select_existing_conversation` below) only
/// makes sense for the native Oz-harness conversation loop: entering Agent
/// View on a CLI-subagent conversation (Claude Code, Codex, ...) isn't
/// something the user can continue querying the same way. A non-Oz harness
/// therefore makes the entry `Unavailable` rather than `Available`, mirroring
/// the TUI's identical rule
/// (`crates/warp_tui/src/conversation_selection.rs::classify_conversation_list_entry`,
/// its `harness != Some(Harness::Oz)` check). Unlike that function's other two
/// conditions (`is_cloud_agent_run`, `has_server_token`), which are
/// permanently false/`None` for every BYOP-local entry and were dropped as
/// genuinely cloud-only, harness is not: `AIConversation::orchestration_harness`
/// already lets a local orchestration child use a non-Oz harness (see
/// `start_new_child_conversation`), so this can become reachable once
/// `AgentConversationsModel::get_entries`'s harness resolution (currently
/// hard-coded to `Some(Harness::Oz)` for every conversation, unrelated to
/// this fix) threads that through.
fn classify_gui_list_entry(
    selected_entry_id: Option<AgentConversationEntryId>,
    entry_id: AgentConversationEntryId,
    open_terminal_view_id: Option<EntityId>,
    terminal_view_id: EntityId,
    harness: Option<Harness>,
) -> AgentConversationListEntryState {
    if selected_entry_id == Some(entry_id) {
        return AgentConversationListEntryState::Selected;
    }
    if open_terminal_view_id.is_some_and(|open_id| open_id != terminal_view_id) {
        return AgentConversationListEntryState::OpenElsewhere;
    }
    if harness != Some(Harness::Oz) {
        return AgentConversationListEntryState::Unavailable;
    }
    AgentConversationListEntryState::Available
}

/// Classifies entries relative to this GUI Agent View terminal view.
impl AgentConversationListPolicy for AgentViewConversationSelection {
    fn classify_entry(
        &self,
        entry: &AgentConversationEntry,
        app: &AppContext,
    ) -> AgentConversationListEntryState {
        let selected_entry_id = self
            .selected_conversation_id(app)
            .map(AgentConversationEntryId::Conversation);
        let history_model = BlocklistAIHistoryModel::as_ref(app);
        let open_terminal_view_id = match entry.id {
            AgentConversationEntryId::AmbientRun(task_id) => {
                history_model.terminal_view_id_for_ambient_task(task_id)
            }
            AgentConversationEntryId::Conversation(conversation_id) => {
                history_model.terminal_view_id_for_conversation(&conversation_id)
            }
        };
        classify_gui_list_entry(
            selected_entry_id,
            entry.id,
            open_terminal_view_id,
            self.terminal_view_id,
            entry.display.harness,
        )
    }
}

impl ConversationSelection for AgentViewConversationSelection {
    fn selected_conversation_id(&self, app: &AppContext) -> Option<AIConversationId> {
        self.agent_view_controller
            .as_ref(app)
            .agent_view_state()
            .active_conversation_id()
    }

    fn is_conversation_active(&self, app: &AppContext) -> bool {
        self.agent_view_controller.as_ref(app).is_active()
    }

    fn is_conversation_fullscreen(&self, app: &AppContext) -> bool {
        self.agent_view_controller.as_ref(app).is_fullscreen()
    }

    fn select_existing_conversation(
        &mut self,
        conversation_id: AIConversationId,
        origin: AgentViewEntryOrigin,
        ctx: &mut ModelContext<Box<dyn ConversationSelection>>,
    ) {
        if let Err(error) = self.agent_view_controller.update(ctx, |controller, ctx| {
            controller.try_enter_agent_view(Some(conversation_id), origin, ctx)
        }) {
            report_error!(
                anyhow::Error::new(error)
                    .context("Failed to enter agent view for existing conversation")
            );
        }
    }

    fn select_new_conversation(
        &mut self,
        origin: AgentViewEntryOrigin,
        ctx: &mut ModelContext<Box<dyn ConversationSelection>>,
    ) {
        if let Err(error) = self.agent_view_controller.update(ctx, |controller, ctx| {
            controller.try_enter_agent_view(None, origin, ctx)
        }) {
            report_error!(
                anyhow::Error::new(error)
                    .context("Failed to enter agent view for new conversation")
            );
        }
    }

    fn try_start_new_conversation(
        &mut self,
        origin: AgentViewEntryOrigin,
        ctx: &mut ModelContext<Box<dyn ConversationSelection>>,
    ) -> Result<AIConversationId, EnterAgentViewError> {
        self.agent_view_controller.update(ctx, |controller, ctx| {
            controller.try_enter_agent_view(None, origin, ctx)
        })
    }

    fn pending_query_autoexecute_override(
        &self,
        app: &AppContext,
    ) -> AIConversationAutoexecuteMode {
        self.selected_conversation_id(app)
            .as_ref()
            .and_then(|conversation_id| {
                BlocklistAIHistoryModel::as_ref(app).conversation(conversation_id)
            })
            .map(|conversation| conversation.autoexecute_override())
            .unwrap_or_default()
    }

    fn toggle_pending_query_autoexecute(
        &mut self,
        ctx: &mut ModelContext<Box<dyn ConversationSelection>>,
    ) {
        if let Some(conversation_id) = self.selected_conversation_id(ctx) {
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.toggle_autoexecute_override(&conversation_id, self.terminal_view_id, ctx);
            });
        }
    }

    fn handle_history_event(
        &mut self,
        event: &BlocklistAIHistoryEvent,
        ctx: &mut ModelContext<Box<dyn ConversationSelection>>,
    ) {
        if event
            .terminal_view_id()
            .is_some_and(|id| id != self.terminal_view_id)
        {
            return;
        }
        match event {
            BlocklistAIHistoryEvent::ClearedConversationsInTerminalView { .. } => {
                self.agent_view_controller
                    .update(ctx, |controller, ctx| controller.exit_agent_view(ctx));
            }
            BlocklistAIHistoryEvent::SplitConversation {
                old_conversation_id,
                new_conversation_id,
                ..
            } if self.selected_conversation_id(ctx) == Some(*old_conversation_id) => {
                self.select_existing_conversation(
                    *new_conversation_id,
                    AgentViewEntryOrigin::AgentRequestedNewConversation,
                    ctx,
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "conversation_selection_tests.rs"]
mod tests;
