//! [`TuiPaneGroup`]: the TUI's structural analog of the GUI's `PaneGroup`
//! (`app/src/pane_group/mod.rs`), scoped to what child-agent panes need.
//!
//! The GUI's `PaneGroup` cannot be instantiated by this crate: it is deeply
//! tied to `warpui`'s GUI view types (`ViewHandle<TerminalView>`,
//! drag-to-resize borders, tab-bar hover state, banners) that have no TUI
//! equivalent. What this type ports instead is the *role* `PaneGroup` plays
//! for child agents specifically -- owning the map from a child-agent
//! conversation to the (hidden) session running it
//! (`PaneGroup::child_agent_panes`), and materializing new children on
//! request (`app/src/pane_group/child_agent/mod.rs`'s
//! `create_hidden_child_agent_conversation`, and
//! `app/src/pane_group/pane/terminal_pane.rs`'s
//! `spawn_local_child_agents`/`finish_spawning_local_child_agent`, which is
//! what `/orchestrate` (`#325`) drives on the GUI side).
//!
//! Concretely, "pane" in the TUI's flat [`TuiSessions`] registry is a
//! [`TuiSessionId`] rather than a `PaneId` in a layout tree: every registered
//! session is already independently focusable, so a "hidden child agent
//! pane" here is simply a [`TuiSessions::create_local_terminal_session_with_env`]
//! session that is never given focus -- it is reachable only by navigating
//! the orchestration tab bar (`crate::orchestration_model`), exactly as a
//! GUI hidden child-agent pane is reachable only from the parent's status
//! card, never the visible split layout.
//!
//! The actual harness launch (validating the `claude` CLI, composing its
//! prompt, building its command line, propagating `OZ_RUN_ID`/
//! `OZ_PARENT_RUN_ID`/model env vars) is *not* duplicated here: it is
//! `app/src/pane_group/pane/local_harness_launch.rs`'s logic, reached
//! through a narrow `pub(crate)` seam added in `app/src/pane_group/pane/mod.rs`
//! and re-exported via `tui_export` -- see that seam's module doc for why a
//! new wrapper was the right move instead of widening
//! `local_harness_launch`'s existing `pub(super)` visibility.
//!
//! The UI trigger lives in `TuiTerminalSessionView::execute_tui_slash_command`
//! (`crates/warp_tui/src/terminal_session_view.rs`): a name-guarded arm on
//! `SlashCommandKind::Other` (matching `commands::ORCHESTRATE.name`, since
//! `/orchestrate` deliberately keeps `kind() == Other` -- see that static's
//! doc comment) validates the active conversation and task argument, then
//! calls [`TuiPaneGroup::spawn_local_child_agents`].
//!
//! Not yet ported from the GUI: per-pane working-directory/shell inheritance
//! (this always inherits the *process's* cwd/`$SHELL`, not the specific
//! session `/orchestrate` was typed in -- the TUI has no per-session cwd
//! tracking the way `PaneGroup::startup_path_for_new_session` does), and
//! `inherit_child_agent_settings` (per-conversation AI profile inheritance).

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

use std::collections::HashMap;

use warp::tui_export::{
    AIConversationId, BlocklistAIHistoryEvent, BlocklistAIHistoryModel, Harness,
    TuiPreparedChildAgentLaunch, prepare_tui_child_agent_launch, tui_compose_child_agent_prompt,
    tui_split_orchestrate_tasks,
};
use warp_terminal::shell::ShellType;
use warpui::SingletonEntity;
use warpui_core::{AppContext, Entity, ModelContext, ModelHandle, ViewHandle, WindowId};

use crate::session_registry::{TuiSessionId, TuiSessions};
use crate::terminal_session_view::TuiTerminalSessionView;

/// The TUI's child-agent pane-group singleton. See the module doc above for
/// how this relates to the GUI's `PaneGroup`.
pub(crate) struct TuiPaneGroup {
    /// Maps child-agent conversation IDs to the hidden session running them.
    /// Mirrors the GUI's `PaneGroup::child_agent_panes`.
    child_agent_sessions: HashMap<AIConversationId, TuiSessionId>,
}

impl Entity for TuiPaneGroup {
    type Event = ();
}

impl SingletonEntity for TuiPaneGroup {}

impl TuiPaneGroup {
    /// Registers the singleton and keeps `child_agent_sessions` from leaking
    /// entries once their conversation is removed or deleted (a killed or
    /// cleared child agent should not keep "occupying" its tracked slot).
    pub(crate) fn register(ctx: &mut AppContext) -> ModelHandle<Self> {
        let history = BlocklistAIHistoryModel::handle(ctx);
        let model = ctx.add_singleton_model(|_| Self {
            child_agent_sessions: HashMap::new(),
        });
        let model_for_history = model.clone();
        ctx.subscribe_to_model(&history, move |_, event, ctx| {
            if let Some(conversation_id) = removed_conversation_id(event) {
                model_for_history.update(ctx, |group, _| {
                    group.child_agent_sessions.remove(&conversation_id);
                });
            }
        });
        model
    }

    /// The hidden session tracked for a child-agent conversation, if any.
    /// Mirrors the GUI's `PaneGroup::child_agent_panes` lookup used to
    /// resolve "reveal this child" navigation targets.
    pub(crate) fn session_for_conversation(
        &self,
        conversation_id: AIConversationId,
    ) -> Option<TuiSessionId> {
        self.child_agent_sessions.get(&conversation_id).copied()
    }

    /// Handles a `/orchestrate`-style argument (`;`-separated tasks): prepares
    /// and materializes one local Claude child agent per task. Mirrors the
    /// GUI's `pane_group::pane::terminal_pane::spawn_local_child_agents`.
    pub(crate) fn spawn_local_child_agents(
        &mut self,
        sessions: &ModelHandle<TuiSessions>,
        window_id: WindowId,
        parent_conversation_id: AIConversationId,
        argument: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        let tasks = tui_split_orchestrate_tasks(argument);
        if tasks.is_empty() {
            log::warn!("TuiPaneGroup::spawn_local_child_agents: no tasks parsed from {argument:?}");
            return;
        }
        let parent_run_id = BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(&parent_conversation_id)
            .and_then(|conversation| conversation.agent_link_id());
        // Simplification vs the GUI (see module doc): inherits the process's
        // own shell/cwd rather than the specific session `/orchestrate` was
        // typed in, since the TUI has no per-session cwd tracking equivalent
        // to `PaneGroup::startup_path_for_new_session`.
        let shell_type = default_shell_type();
        let startup_directory = std::env::current_dir().ok();

        for task in tasks {
            let prompt = tui_compose_child_agent_prompt(&task);
            if prompt.is_empty() {
                continue;
            }
            let agent_name = prompt.clone();
            let future = prepare_tui_child_agent_launch(
                prompt,
                parent_run_id.clone(),
                shell_type,
                startup_directory.clone(),
            );
            let sessions = sessions.clone();
            ctx.spawn(future, move |pane_group, result, ctx| match result {
                Ok(prepared) => {
                    // Creating the real, hidden PTY-backed session lives here
                    // (not in `finish_spawning_local_child_agent`) precisely
                    // so that method's conversation-registration/PTY-write
                    // logic stays unit-testable against a mock session --
                    // see its doc comment.
                    let (session_id, view) = TuiSessions::create_local_terminal_session_with_env(
                        &sessions,
                        window_id,
                        // Hidden: only ever reachable by navigating the
                        // orchestration tab bar, mirroring the GUI's
                        // `NewPaneVisibility::HiddenForChildAgent`.
                        false,
                        None,
                        prepared.env_vars.clone(),
                        ctx,
                    );
                    pane_group.finish_spawning_local_child_agent(
                        session_id,
                        &view,
                        parent_conversation_id,
                        agent_name,
                        &prepared,
                        ctx,
                    );
                }
                Err(message) => {
                    log::error!(
                        "TuiPaneGroup::spawn_local_child_agents: failed to prepare local child launch: {message}"
                    );
                }
            });
        }
    }

    /// Materializes one spawned child agent onto an already-created session:
    /// registers its conversation in the orchestration topology and types
    /// the harness command into its PTY. Mirrors the GUI's
    /// `finish_spawning_local_child_agent`
    /// (`app/src/pane_group/pane/terminal_pane.rs`), minus that function's
    /// own pane-creation step (split out into
    /// [`Self::spawn_local_child_agents`]'s caller) so this half -- the part
    /// that actually has TUI-specific behavior -- is testable against a mock
    /// session the way [`crate::terminal_session_view_tests`]'s
    /// `add_orchestration_child` fixture already exercises for plain
    /// (non-agent-launched) children.
    fn finish_spawning_local_child_agent(
        &mut self,
        session_id: TuiSessionId,
        view: &ViewHandle<TuiTerminalSessionView>,
        parent_conversation_id: AIConversationId,
        agent_name: String,
        prepared: &TuiPreparedChildAgentLaunch,
        ctx: &mut ModelContext<Self>,
    ) -> AIConversationId {
        // The child's own view id is its `terminal_view_id`, matching how a
        // brand-new top-level conversation is always registered under its
        // own view -- not the parent's. See
        // `TuiSessions::session_ids_by_conversation`, which the orchestration
        // snapshot depends on to resolve which session backs which
        // conversation.
        let child_id = BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            let child_id = history.start_new_child_conversation(
                session_id.surface_id(),
                agent_name,
                parent_conversation_id,
                Some(Harness::Claude),
                ctx,
            );
            history.assign_run_id_for_conversation(
                child_id,
                prepared.run_id.clone(),
                Some(prepared.task_id),
                session_id.surface_id(),
                ctx,
            );
            history.set_active_conversation_id(child_id, session_id.surface_id(), ctx);
            child_id
        });

        view.update(ctx, |view, ctx| {
            view.write_child_harness_command(&prepared.command, ctx);
        });

        self.child_agent_sessions.insert(child_id, session_id);
        child_id
    }

    /// Permanently discards the hidden session backing a child-agent
    /// conversation. Mirrors the GUI's
    /// `PaneGroup::discard_child_agent_pane_for_conversation`. Returns
    /// `false` if no session is tracked for `conversation_id`.
    pub(crate) fn discard_child_agent_session_for_conversation(
        &mut self,
        sessions: &ModelHandle<TuiSessions>,
        conversation_id: AIConversationId,
        ctx: &mut AppContext,
    ) -> bool {
        let Some(session_id) = self.child_agent_sessions.remove(&conversation_id) else {
            return false;
        };
        sessions.update(ctx, |sessions, ctx| {
            sessions.remove_session(session_id, ctx);
        });
        true
    }
}

/// Detects the type of the user's own default shell from `$SHELL`, the same
/// signal `local_harness_launch::validate_local_harness_shell` gates local
/// child launches on. Returns `None` (rejected by that validation) for an
/// unset/unrecognized `$SHELL` rather than guessing.
fn default_shell_type() -> Option<ShellType> {
    std::env::var("SHELL")
        .ok()
        .and_then(|shell| ShellType::from_name(&shell))
}

fn removed_conversation_id(event: &BlocklistAIHistoryEvent) -> Option<AIConversationId> {
    match event {
        BlocklistAIHistoryEvent::RemoveConversation {
            conversation_id, ..
        }
        | BlocklistAIHistoryEvent::DeletedConversation {
            conversation_id, ..
        } => Some(*conversation_id),
        _ => None,
    }
}

#[cfg(test)]
#[path = "pane_group_tests.rs"]
mod tests;
