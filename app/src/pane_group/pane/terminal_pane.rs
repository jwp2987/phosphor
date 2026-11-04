//! Implementation of terminal panes.
#[cfg(feature = "local_fs")]
use crate::pane_group::CodeSource;
use std::sync::mpsc::SyncSender;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

use warpui::{
    AppContext, EntityId, ModelHandle, SingletonEntity, ViewContext, ViewHandle, WindowId,
};

use crate::{
    AIExecutionProfilesModel,
    ai::{
        agent::conversation::AIConversationId,
        blocklist::{BlocklistAIHistoryModel, agent_view::AgentViewControllerEvent},
        llms::LLMPreferences,
    },
    app_state::{AmbientAgentPaneSnapshot, LeafContents, TerminalPaneSnapshot},
    pane_group::{self, Direction, Event::OpenConversationHistory, PaneGroup},
    persistence::{BlockCompleted, ModelEvent},
    session_management::SessionNavigationData,
    terminal::cli_agent_sessions::CLIAgentSessionsModel,
    terminal::{
        TerminalManager, TerminalView, general_settings::GeneralSettings,
        shared_session::SharedSessionStatus, view::Event,
    },
    view_components::ToastFlavor,
    workspace::{PaneViewLocator, WorkspaceRegistry, sync_inputs::SyncedInputState},
};

#[cfg(feature = "local_fs")]
use crate::ai::blocklist::BlocklistAIHistoryEvent;

use crate::ai::blocklist::agent_view::AgentViewEntryOrigin;
use warp_cli::agent::Harness;

use warp_core::execution_mode::AppExecutionMode;

use crate::terminal::ShellLaunchData;

use super::{
    DetachType, PaneConfiguration, PaneContent, PaneId, PaneStackEvent, PaneView, ShareableLink,
    ShareableLinkError, TerminalPaneId,
};

pub type TerminalPaneView = PaneView<TerminalView>;

/// Data kept for terminal panes.
pub struct TerminalPane {
    model_event_sender: Option<SyncSender<ModelEvent>>,

    /// Used to uniquely identify the pane, even across separate runs of the app.
    uuid: Vec<u8>,

    pane_configuration: ModelHandle<PaneConfiguration>,

    /// Defining `terminal_manager` before `view` means that `terminal_manager`
    /// gets dropped first (guaranteed by the language), which halts the event
    /// loop and avoids possible deadlocks during session cleanup. This is enforced
    /// by the `PaneStack`, since the terminal manager is the associated data for
    /// the backing pane view.
    view: ViewHandle<TerminalPaneView>,

    /// The persisted state this pane was restored from, if it was restored
    /// from a session snapshot (#1096). Until the restored shell reports a
    /// session (bootstraps), the live view knows neither its working
    /// directory nor its shell, so [`PaneContent::snapshot`] re-writes these
    /// persisted values instead of overwriting them with "unknown" -- a quit
    /// within a couple of seconds of launch would otherwise permanently reset
    /// every not-yet-loaded restored tab to a default `~` session.
    restored_state: Option<RestoredTerminalState>,

    /// Set while a shell restarted in place (#1103) hands this pane's session uuid to its
    /// replacement: the pane closed by the swap must not delete the blocks saved under it, which
    /// the replacement now owns. See [`PaneGroup::restart_exited_shell`].
    keep_persisted_blocks_on_close: std::cell::Cell<bool>,
}

/// The parts of a [`TerminalPaneSnapshot`] that only the shell can report
/// back, kept from the snapshot a pane was restored from. See
/// [`TerminalPane::restored_state`].
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::pane_group) struct RestoredTerminalState {
    pub cwd: Option<String>,
    pub shell_launch_data: Option<ShellLaunchData>,
}

fn serialize_proto_to_base64<M: prost::Message>(message: &M) -> String {
    BASE64_STANDARD.encode(message.encode_to_vec())
}

impl TerminalPane {
    pub(in crate::pane_group) fn new(
        uuid: Vec<u8>,
        terminal_manager: ModelHandle<Box<dyn TerminalManager>>,
        terminal_view: ViewHandle<TerminalView>,
        model_event_sender: Option<SyncSender<ModelEvent>>,
        ctx: &mut ViewContext<PaneGroup>,
    ) -> Self {
        let pane_configuration = terminal_view.as_ref(ctx).pane_configuration().to_owned();
        let view = ctx.add_typed_action_view(|ctx| {
            let pane_id = PaneId::from_terminal_pane_ctx(ctx);
            PaneView::new(
                pane_id,
                terminal_view,
                terminal_manager,
                pane_configuration.clone(),
                ctx,
            )
        });

        Self {
            model_event_sender,
            uuid,
            pane_configuration,
            view,
            restored_state: None,
            keep_persisted_blocks_on_close: std::cell::Cell::new(false),
        }
    }

    /// See [`Self::keep_persisted_blocks_on_close`].
    pub(in crate::pane_group) fn set_keep_persisted_blocks_on_close(&self, keep: bool) {
        self.keep_persisted_blocks_on_close.set(keep);
    }

    /// Records the persisted state this pane was restored from, so a snapshot
    /// taken before its shell bootstraps preserves it (#1096).
    pub(in crate::pane_group) fn set_restored_state(&mut self, state: RestoredTerminalState) {
        self.restored_state = Some(state);
    }

    /// The [`PaneView<TerminalView>`] for this pane.
    #[cfg(any(test, feature = "integration_tests"))]
    pub(in crate::pane_group) fn pane_view(&self) -> ViewHandle<TerminalPaneView> {
        self.view.to_owned()
    }

    /// The [`TerminalView`] backing the [`PaneView`] for this terminal pane.
    pub(crate) fn terminal_view(&self, ctx: &AppContext) -> ViewHandle<TerminalView> {
        self.view.as_ref(ctx).child(ctx)
    }

    /// The UUID that identifies this terminal session across app restarts.
    pub(in crate::pane_group) fn session_uuid(&self) -> Vec<u8> {
        self.uuid.clone()
    }

    /// The terminal manager responsible for this session's event loop.
    pub(in crate::pane_group) fn terminal_manager(
        &self,
        ctx: &AppContext,
    ) -> ModelHandle<Box<dyn TerminalManager>> {
        self.view.as_ref(ctx).child_data(ctx).clone()
    }

    /// Instructs the SQLite thread to delete blocks for this session.
    pub(in crate::pane_group) fn delete_blocks(&self, ctx: &AppContext) {
        if !AppExecutionMode::as_ref(ctx).can_save_session() {
            return;
        }

        if let Some(sender) = &self.model_event_sender {
            let model_event = ModelEvent::DeleteBlocks(self.uuid.clone());
            if let Err(err) = sender.send(model_event) {
                log::error!(
                    "Error sending blocks deleted event for terminal id {} {:?}",
                    self.terminal_view(ctx).id(),
                    err
                );
            }
        }
    }

    pub fn session_navigation_data(
        &self,
        pane_group_id: EntityId,
        window_id: WindowId,
        app: &AppContext,
    ) -> SessionNavigationData {
        let view = self.terminal_view(app).as_ref(app);
        SessionNavigationData::new(
            view.full_prompt(app),
            view.prompt_elements(app),
            view.session_command_context(app),
            PaneViewLocator {
                pane_group_id,
                pane_id: self.id(),
            },
            view.last_focus_ts(),
            view.is_read_only(),
            window_id,
            view.model.lock().shared_session_status().clone(),
        )
    }

    pub fn terminal_pane_id(&self) -> TerminalPaneId {
        self.id()
            .as_terminal_pane_id()
            .expect("Should be able to derive a TerminalPaneId from TerminalPane")
    }
}

impl PaneContent for TerminalPane {
    fn id(&self) -> PaneId {
        PaneId::from_terminal_pane_view(&self.view)
    }

    fn attach(
        &self,
        group: &PaneGroup,
        focus_handle: crate::pane_group::focus_state::PaneFocusHandle,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        // TODO(ben): As much as possible, logic from PaneGroup::add_session should go here.
        //  This will simplify PaneGroup, especially when implementing pane management.
        let terminal_pane_id = self.terminal_pane_id();

        self.view
            .update(ctx, |view, ctx| view.set_focus_handle(focus_handle, ctx));

        // A restored (undo-closed) pane may start agent work again. Nothing resumes on its own:
        // conversations stopped by the close stay cancelled and their queues wait for the user
        // (#848; see `TerminalView::suspend_agent_work_for_close`). Not for a pane that is
        // itself still closed: `reattach_panes` (undoing a tab close) attaches every pane of the
        // group, including one closed earlier inside it, which stays in the undo stack and
        // hidden. `restore_closed_pane` unhides a pane before attaching it.
        let is_still_closed = group.is_pane_hidden_for_close(terminal_pane_id.into());
        let views = self
            .view
            .as_ref(ctx)
            .pane_stack()
            .as_ref(ctx)
            .entries()
            .iter()
            .map(|(_, view)| view.clone())
            .collect::<Vec<_>>();
        if !is_still_closed {
            for view in views {
                view.update(ctx, |view, ctx| view.resume_after_close_undone(ctx));
            }
        }

        // Attach the initial terminal view in the stack.
        attach_terminal_view(&self.terminal_view(ctx), terminal_pane_id, ctx);

        // Subscribe to the pane stack to handle views being pushed/popped.
        let pane_stack = self.view.as_ref(ctx).pane_stack().clone();
        ctx.subscribe_to_model(&pane_stack, move |group, _, event, ctx| {
            handle_pane_stack_event(group, event, terminal_pane_id, ctx);
        });

        ctx.subscribe_to_view(&self.view, move |group, _, event, ctx| {
            group.handle_pane_view_event(terminal_pane_id.into(), event, ctx);
        });

        if SyncedInputState::as_ref(ctx).should_sync_this_pane_group(ctx.view_id(), ctx.window_id())
        {
            if let Some(active_pane_view) = group.active_session_view(ctx) {
                let event = active_pane_view
                    .as_ref(ctx)
                    .create_sync_event_based_on_terminal_state(ctx);

                group.send_sync_event_to_session(terminal_pane_id, &event, ctx);
            }
        }

        let terminal_view_id = self.terminal_view(ctx).id();

        #[cfg(feature = "local_fs")]
        {
            // Streaming exchanges already persisted with their current status (#1137).
            let persisted_pending: std::rc::Rc<std::cell::RefCell<PersistedPendingExchanges>> =
                Default::default();
            ctx.subscribe_to_model(
                &BlocklistAIHistoryModel::handle(ctx),
                move |group, _, event, ctx| {
                    let Some(model_event_sender) = group.model_event_sender.clone() else {
                        return;
                    };

                    let is_shared_ambient_agent_session = group
                        .terminal_view_from_pane_id(terminal_pane_id, ctx)
                        .map(|view| {
                            view.as_ref(ctx)
                                .model
                                .lock()
                                .is_shared_ambient_agent_session()
                        })
                        .unwrap_or(false);

                    handle_ai_history_event(
                        event,
                        terminal_view_id,
                        terminal_pane_id,
                        model_event_sender,
                        is_shared_ambient_agent_session,
                        &mut persisted_pending.borrow_mut(),
                        ctx,
                    );
                },
            );
        }

        // Store the pane group entity ID on the agent view controller so the
        // message bar can perform pane-group-scoped visibility checks.
        let pane_group_id = ctx.view_id();
        let terminal_view = self.terminal_view(ctx);
        let agent_view_controller = terminal_view.as_ref(ctx).agent_view_controller().clone();
        agent_view_controller.update(ctx, |controller, _ctx| {
            controller.set_pane_group_id(pane_group_id);
        });

        // Lazy hidden-child-agent restoration. Entering a *fullscreen* agent
        // view is the moment the orchestration pill bar becomes visible, so it
        // is also the moment the parent's children need real panes behind
        // them. Before this subscription existed, child panes were only ever
        // built by the eager `PaneGroup::create_missing_child_agent_panes`
        // sweep at construction, which meant a child agent restored from
        // SQLite had no pane at all until the *next* cold start.
        // Mirrors the pin's subscription in `TerminalPane::attach`; the
        // matching `unsubscribe_to_model` is in `detach` below, so a detached
        // (hidden-for-close) tab does not materialize panes behind the user's
        // back.
        ctx.subscribe_to_model(&agent_view_controller, move |group, _, event, ctx| {
            if let AgentViewControllerEvent::EnteredAgentView {
                conversation_id,
                display_mode,
                ..
            } = event
                && display_mode.is_fullscreen()
            {
                group.restore_missing_child_agent_panes_for_parent(
                    *conversation_id,
                    terminal_pane_id.into(),
                    ctx,
                );
            }
        });

        let _ = terminal_view_id;
    }

    fn detach(
        &self,
        _group: &PaneGroup,
        detach_type: DetachType,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        // Closing a pane -- restorably or for good -- stops its agent work (#848). Without
        // this, a hidden-for-close pane kept streaming, drained its queued prompts at the turn
        // end and ran queued commands in the detached shell, and a permanent close cancelled
        // only the *active* conversation. Must run before the conversations are cleared below,
        // while they can still be found by terminal view. A move keeps everything running.
        if matches!(detach_type, DetachType::HiddenForClose | DetachType::Closed) {
            let views = self
                .view
                .as_ref(ctx)
                .pane_stack()
                .as_ref(ctx)
                .entries()
                .iter()
                .map(|(_, view)| view.clone())
                .collect::<Vec<_>>();
            for view in views {
                view.update(ctx, |view, ctx| view.suspend_agent_work_for_close(ctx));
            }
        }

        if matches!(detach_type, DetachType::Closed) {
            // Only immediately clear conversations and delete blocks if the session is being
            // permanently closed.
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
                history_model
                    .clear_conversations_in_terminal_view(self.terminal_view(ctx).id(), ctx);
            });
            // A shell restarted in place (#1103) hands the uuid, and the blocks saved under it,
            // to the pane that replaces this one.
            if !self.keep_persisted_blocks_on_close.get() {
                self.delete_blocks(ctx);
            }
            // Stop the pty and its process tree (local shell, ssh client/ControlMaster). The
            // pane is gone for good, so nothing may keep running behind it (#980).
            let views = self
                .view
                .as_ref(ctx)
                .pane_stack()
                .as_ref(ctx)
                .entries()
                .iter()
                .map(|(_, view)| view.clone())
                .collect::<Vec<_>>();
            for view in views {
                view.update(ctx, |view, ctx| view.shutdown_for_discard(ctx));
            }
        }

        // Unsubscribe from all views in the pane stack.
        let pane_stack = self.view.as_ref(ctx).pane_stack().clone();
        let contents = pane_stack.as_ref(ctx).entries().to_vec();
        for (manager, view) in contents {
            // Notify the view that it's being detached so it can react appropriately
            // (e.g. the shared-session viewer tears down its network only when the detach
            // is not reversible).
            manager.update(ctx, |terminal_manager, ctx| {
                terminal_manager.on_view_detached(detach_type, ctx);
            });
            // #1001: a remote session that never finished bootstrapping (wedged or abandoned
            // connect) has nothing to restore, so closing it must not keep its shell, ssh
            // client, ControlMaster and remote proxy alive for the undo-close grace period.
            if detach_type_skips_undo_grace(
                detach_type,
                manager.as_ref(ctx).has_unbootstrapped_remote_session(),
            ) {
                manager.update(ctx, |terminal_manager, ctx| {
                    terminal_manager.discard_now(ctx)
                });
            }
            ctx.unsubscribe_to_view(&view);
        }

        let terminal_view_id = self.terminal_view(ctx).id();

        // Clean up any active CLI agent session so its notification is removed.
        // Skip this for moves — the session is still running and will re-register in the new tab.
        if !matches!(detach_type, DetachType::Moved) {
            CLIAgentSessionsModel::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.remove_session(terminal_view_id, ctx);
            });
        }

        ctx.unsubscribe_to_model(&pane_stack);

        ctx.unsubscribe_to_view(&self.view);

        // Drops the `EnteredAgentView` subscription installed by `attach`.
        // Without this, a detached (hidden-for-close) pane would keep
        // materializing hidden child panes into a pane group it is no longer
        // part of; `reattach_panes` reinstalls it and re-runs the restoration
        // explicitly.
        ctx.unsubscribe_to_model(
            &self
                .terminal_view(ctx)
                .as_ref(ctx)
                .agent_view_controller()
                .clone(),
        );

        #[cfg(feature = "local_fs")]
        {
            ctx.unsubscribe_to_model(&BlocklistAIHistoryModel::handle(ctx));
        }
    }

    fn snapshot(&self, app: &AppContext) -> LeafContents {
        let view = self.terminal_view(app).as_ref(app);
        let is_active = view.is_active_session(app);

        // Capture the current input_config from the AI input model
        let current_input_config = view.input_config(app.as_ref());

        if view.model.lock().shared_session_status().is_viewer() {
            // We save and restore ambient agent sessions
            // (restoring the shared session if it's still open and the conversation transcript otherwise).
            let ambient_model = view.ambient_agent_view_model().as_ref(app);
            if ambient_model.is_ambient_agent() {
                let task_id = ambient_model.task_id();

                return LeafContents::AmbientAgent(AmbientAgentPaneSnapshot {
                    uuid: self.uuid.clone(),
                    task_id,
                });
            }

            LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: self.uuid.clone(),
                cwd: None,
                is_active,
                is_read_only: false,
                shell_launch_data: None,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
            })
        } else if view.model.lock().is_conversation_transcript_viewer() {
            // Conversation transcript viewers (opened from the conversation list)
            // can be restored via the ambient agent task if one exists.
            let task_id = view.model.lock().ambient_agent_task_id();
            if task_id.is_some() {
                LeafContents::AmbientAgent(AmbientAgentPaneSnapshot {
                    uuid: self.uuid.clone(),
                    task_id,
                })
            } else {
                LeafContents::Terminal(TerminalPaneSnapshot {
                    uuid: self.uuid.clone(),
                    cwd: None,
                    is_active,
                    is_read_only: false,
                    shell_launch_data: None,
                    input_config: None,
                    llm_model_override: None,
                    active_profile_id: None,
                    conversation_ids_to_restore: vec![],
                    active_conversation_id: None,
                })
            }
        } else {
            let llm_model_override =
                LLMPreferences::as_ref(app).get_base_llm_override(self.terminal_view(app).id());

            let active_profile_id = AIExecutionProfilesModel::as_ref(app)
                .active_profile(Some(self.terminal_view(app).id()), app)
                .sync_id();

            // Collect all conversation IDs for this terminal view
            let conversation_ids_to_restore = BlocklistAIHistoryModel::as_ref(app)
                .all_live_conversations_for_terminal_view(self.terminal_view(app).id())
                .map(|conversation| conversation.id())
                .collect();

            // Capture agent view state: if fullscreen, store the active conversation ID
            let active_conversation_id = view
                .agent_view_controller()
                .as_ref(app)
                .agent_view_state()
                .display_mode()
                .filter(|mode| mode.is_fullscreen())
                .and_then(|_| {
                    view.agent_view_controller()
                        .as_ref(app)
                        .agent_view_state()
                        .active_conversation_id()
                });

            // #1096: a restored pane whose shell has not reported a session
            // yet has no live cwd or shell; keep the persisted ones rather
            // than overwrite them with `None` (which restores as a default
            // `~` session). Once the shell has bootstrapped, the live values
            // win -- including `None` for a session that went remote.
            let (cwd, shell_launch_data) = match &self.restored_state {
                Some(restored) if view.active_session_is_local(app).is_none() => {
                    (restored.cwd.clone(), restored.shell_launch_data.clone())
                }
                _ => (view.pwd_if_local(app), view.shell_launch_data_if_local(app)),
            };

            LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: self.uuid.clone(),
                cwd,
                is_active,
                is_read_only: view.model.lock().is_read_only(),
                shell_launch_data,
                input_config: Some(current_input_config),
                llm_model_override,
                active_profile_id,
                conversation_ids_to_restore,
                active_conversation_id,
            })
        }
    }

    fn has_application_focus(&self, ctx: &mut ViewContext<PaneGroup>) -> bool {
        self.view.is_self_or_child_focused(ctx)
    }

    fn focus(&self, ctx: &mut ViewContext<PaneGroup>) {
        self.terminal_view(ctx)
            .update(ctx, |view, ctx| view.redetermine_global_focus(ctx));
    }

    fn shareable_link(
        &self,
        ctx: &mut ViewContext<PaneGroup>,
    ) -> Result<ShareableLink, ShareableLinkError> {
        let manager = self.terminal_manager(ctx);
        let the_model = manager.as_ref(ctx).model();
        let lock = the_model.lock();

        // Check if this is a conversation transcript viewer
        if lock.is_conversation_transcript_viewer() {
            // Try to get the conversation token from the history model
            let history_model = crate::ai::blocklist::BlocklistAIHistoryModel::handle(ctx);
            let terminal_view_id = self.terminal_view(ctx).id();

            // Find the conversation for this terminal view
            // We're assuming the conversation transcript view only has one conversation.
            // TODO(roland): store conversation id or server conversation token on the model ConversationTranscriptViewerStatus
            if let Some(conversation) = history_model
                .as_ref(ctx)
                .all_live_conversations_for_terminal_view(terminal_view_id)
                .next()
            {
                if let Some(token) = conversation.server_conversation_token() {
                    let url_string = token.conversation_link();
                    if let Ok(url) = url::Url::parse(&url_string) {
                        return Ok(ShareableLink::Pane { url });
                    }
                }
            }

            // If we can't get the conversation link yet (still loading or not available),
            // return Expected error to preserve the current browser URL
            return Err(ShareableLinkError::Expected);
        }

        // Check for shared session status
        let session_status = lock.shared_session_status();
        match session_status {
            SharedSessionStatus::NotShared => Ok(ShareableLink::Base),
            SharedSessionStatus::ActiveViewer { role: _ } => Err(ShareableLinkError::Expected),
            _ => Err(ShareableLinkError::Expected),
        }
    }

    fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    fn is_pane_being_dragged(&self, ctx: &AppContext) -> bool {
        self.view.as_ref(ctx).is_being_dragged()
    }
}

/// Attaches a terminal view to the pane group by subscribing to its events
/// and setting the file tree code model.
fn attach_terminal_view(
    terminal_view: &ViewHandle<TerminalView>,
    terminal_pane_id: TerminalPaneId,
    ctx: &mut ViewContext<PaneGroup>,
) {
    ctx.subscribe_to_view(
        terminal_view,
        move |group: &mut PaneGroup, _, event, ctx| {
            handle_terminal_view_event(group, terminal_pane_id, event, ctx);
        },
    );
}

/// Handles events from the pane stack when views are added or removed.
fn handle_pane_stack_event(
    group: &mut PaneGroup,
    event: &PaneStackEvent<TerminalView>,
    terminal_pane_id: TerminalPaneId,
    ctx: &mut ViewContext<PaneGroup>,
) {
    match event {
        PaneStackEvent::ViewAdded(terminal_view) => {
            attach_terminal_view(terminal_view, terminal_pane_id, ctx);
        }
        PaneStackEvent::ViewRemoved(terminal_view) => {
            ctx.unsubscribe_to_view(terminal_view);
        }
    }

    // Ensure we use the new top-level view's title and active session status.
    // TODO(ben): This shouldn't be necessary once titles are set declaratively.
    if let Some(active_terminal) = group.terminal_view_from_pane_id(terminal_pane_id, ctx) {
        active_terminal.update(ctx, |view, ctx| view.on_pane_state_change(ctx));
    }
}

fn handle_terminal_view_event(
    group: &mut PaneGroup,
    terminal_pane_id: TerminalPaneId,
    event: &Event,
    ctx: &mut ViewContext<PaneGroup>,
) {
    let pane_id = terminal_pane_id.into();

    if group.pane_contents.contains_key(&pane_id) {
        match event {
            Event::Escape => ctx.emit(pane_group::Event::Escape),
            Event::ExecuteCommand(event) => {
                ctx.emit(pane_group::Event::ExecuteCommand(event.clone()));
            }
            Event::Exited => {
                let bootstrapped = group
                    .terminal_view_from_pane_id(terminal_pane_id, ctx)
                    .is_none_or(|terminal_view| {
                        terminal_view.as_ref(ctx).is_login_shell_bootstrapped()
                    });
                let keep = exited_shell_keeps_pane(
                    bootstrapped,
                    warpui::platform::session_end_gate::keep_exited_pane_open,
                );
                // If the shell process exited before it successfully bootstrapped,
                // keep the pane open.  There might be useful information visible
                // in the output, and if this was the first shell spawned when the
                // user started the app, it will prevent it from suddenly quitting.
                if keep == ExitedShellKeep::NotBootstrapped {
                    return;
                }

                // Windows kills the console shells when a session end is merely queried, before
                // the user has chosen; closing their panes would quit the app, and a cancelled
                // restart would then have lost everything running in it. Keep the pane (it shows
                // the shell-terminated banner) while a session end is queried, under way (so the
                // saved session still has the tab) or just cancelled (jwp2987/phosphor#1103).
                if keep == ExitedShellKeep::SessionEnd {
                    log::info!("Keeping the pane of an exited shell open: Windows session end");
                    // Windows killed the shell itself, so what ran in it is gone; if the session
                    // end is cancelled the user can restart the shell in place from the banner.
                    // Only their click does it (#1103).
                    if let Some(terminal_view) =
                        group.terminal_view_from_pane_id(terminal_pane_id, ctx)
                    {
                        terminal_view.update(ctx, |view, ctx| view.offer_shell_restart(ctx));
                    }
                    return;
                }

                // Never close a pane out from under an approval card the user has not answered
                // (#1290): the tab would vanish with the card. The shell-terminated banner is
                // already shown; the user closes the pane after resolving the card.
                if group
                    .terminal_view_from_pane_id(terminal_pane_id, ctx)
                    .is_some_and(|terminal_view| {
                        terminal_view.as_ref(ctx).has_pending_agent_action(ctx)
                    })
                {
                    log::debug!(
                        "Keeping pane {pane_id:?} open after its shell exited: an agent approval is pending"
                    );
                    return;
                }

                group.close_pane(pane_id, ctx);
            }
            Event::CloseRequested => {
                group.close_pane_with_confirmation(pane_id, ctx);
            }
            Event::RestartExitedShellRequested => {
                if !group.restart_exited_shell(terminal_pane_id, ctx) {
                    // Nothing was replaced: offer the button again rather than leave a dead pane
                    // with no way forward.
                    if let Some(terminal_view) =
                        group.terminal_view_from_pane_id(terminal_pane_id, ctx)
                    {
                        terminal_view.update(ctx, |view, ctx| view.offer_shell_restart(ctx));
                    }
                }
            }
            Event::Pane(pane_event) => group.handle_pane_event(pane_id, pane_event, ctx),
            Event::BlockListCleared => {
                // Capture CMD-K to clear blocks here so we could remove
                // all the associated blocks stored in the history.
                if let Some(terminal_pane) = group.terminal_session_by_id(pane_id) {
                    terminal_pane.delete_blocks(ctx);
                }
            }
            Event::SendNotification(notification) => {
                ctx.emit(pane_group::Event::SendNotification {
                    notification: notification.clone(),
                    pane_id,
                })
            }
            Event::PluggableNotification { title, body } => {
                let message = if let Some(t) = title {
                    format!("{t}: {body}")
                } else {
                    body.clone()
                };
                ctx.emit(pane_group::Event::ShowToast {
                    message,
                    flavor: ToastFlavor::Default,
                    pane_id: Some(pane_id),
                })
            }
            Event::AppStateChanged => {
                ctx.emit(pane_group::Event::AppStateChanged);
            }
            Event::BlockCompleted {
                block,
                persisted_content,
                is_local,
            } => {
                match group.terminal_session_by_id(pane_id) {
                    Some(pane) => {
                        if *GeneralSettings::as_ref(ctx).restore_session
                            && AppExecutionMode::as_ref(ctx).can_save_session()
                        {
                            if let Some(sender) = &group.model_event_sender {
                                let block_completed_event = ModelEvent::SaveBlock(BlockCompleted {
                                    pane_id: pane.session_uuid(),
                                    block: block.clone(),
                                    deferred_content: persisted_content.clone(),
                                    is_local: *is_local,
                                });

                                let sender_clone = sender.clone();
                                let _ = ctx.spawn(async move {
                                // Sending over a sync sender can block the current thread, so we do this async.
                                sender_clone.send(block_completed_event)
                            }, move |_, res, _| {
                                if let Err(err) = res {
                                    log::error!("Error sending block completed event for terminal id {terminal_pane_id:?} {err:?}");
                                }
                            });
                            }
                        }
                        ctx.emit(pane_group::Event::ActiveSessionChanged);
                    }
                    None => {
                        log::error!("Could not find uuid for terminal id: {terminal_pane_id:?}");
                    }
                };
            }
            Event::SessionBootstrapped => {
                ctx.emit(pane_group::Event::ActiveSessionChanged);
            }
            Event::OpenSettings(section) => {
                ctx.emit(pane_group::Event::OpenSettings(*section));
            }
            Event::OpenPluginInstructionsPane(agent, kind) => {
                ctx.emit(pane_group::Event::OpenPluginInstructionsPane(*agent, *kind));
            }
            Event::SyncInput(sync_event) => {
                if SyncedInputState::as_ref(ctx)
                    .should_sync_this_pane_group(ctx.view_id(), ctx.window_id())
                {
                    ctx.emit(pane_group::Event::SyncInput(sync_event.clone()));
                }
            }
            Event::ShowCommandSearch(options) => {
                ctx.emit(pane_group::Event::ShowCommandSearch(options.clone()));
            }
            Event::TerminalViewStateChanged => {
                ctx.emit(pane_group::Event::TerminalViewStateChanged);
            }
            Event::OnboardingTutorialCompleted => {
                ctx.emit(pane_group::Event::OnboardingTutorialCompleted);
            }
            Event::OpenWorkflowModalWithCommand(command) => {
                ctx.emit(pane_group::Event::OpenWorkflowModalWithCommand(
                    command.clone(),
                ));
            }
            Event::OpenWorkflowModalWithWorkflowObject(workflow_id) => {
                ctx.emit(pane_group::Event::OpenCloudWorkflowForEdit(*workflow_id));
            }
            Event::OpenWorkflowModalWithTemporary(workflow) => {
                ctx.emit(pane_group::Event::OpenWorkflowModalWithTemporary(
                    workflow.clone(),
                ));
            }
            Event::OpenPromptEditor => {
                ctx.emit(pane_group::Event::OpenPromptEditor);
            }
            Event::OpenAgentToolbarEditor => {
                ctx.emit(pane_group::Event::OpenAgentToolbarEditor);
            }
            Event::OpenCLIAgentToolbarEditor => {
                ctx.emit(pane_group::Event::OpenCLIAgentToolbarEditor);
            }
            Event::OpenFileInWarp { path, session } => {
                ctx.emit(pane_group::Event::OpenFileInWarp {
                    path: path.clone(),
                    session: session.clone(),
                });
            }
            #[cfg(feature = "local_fs")]
            Event::PreviewCodeInWarp { source } => {
                ctx.emit(pane_group::Event::PreviewCodeInWarp {
                    source: source.clone(),
                });
            }
            #[cfg(feature = "local_fs")]
            Event::OpenCodeInWarp { source, layout } => {
                ctx.emit(pane_group::Event::OpenCodeInWarp {
                    source: source.clone(),
                    layout: *layout,
                    line_col: if let CodeSource::Link { range_start, .. } = source {
                        *range_start
                    } else {
                        None
                    },
                });
            }
            Event::OpenCodeDiff { view } => {
                ctx.emit(pane_group::Event::OpenCodeDiff { view: view.clone() });
            }
            Event::OpenCodeReviewPane(arg) => {
                ctx.emit(pane_group::Event::OpenCodeReviewPane(arg.clone()));
            }
            Event::OpenCodeReviewPaneAndScrollToComment {
                open_code_review,
                comment,
                diff_mode,
            } => {
                ctx.emit(pane_group::Event::OpenCodeReviewPaneAndScrollToComment {
                    open_code_review: open_code_review.clone(),
                    comment: comment.clone(),
                    diff_mode: diff_mode.clone(),
                });
            }
            Event::ImportAllCodeReviewComments {
                open_code_review,
                comments,
                diff_mode,
            } => {
                ctx.emit(pane_group::Event::ImportAllCodeReviewComments {
                    open_code_review: open_code_review.clone(),
                    comments: comments.clone(),
                    diff_mode: diff_mode.clone(),
                });
            }
            Event::ToggleCodeReviewPane(arg) => {
                ctx.emit(pane_group::Event::ToggleCodeReviewPane(arg.clone()));
            }
            Event::FocusSession => {
                group.focus_pane(terminal_pane_id.into(), true, ctx);
                ctx.emit(pane_group::Event::FocusPaneGroup);
            }
            Event::ZapDriveObjectInPane(uid) => {
                ctx.emit(pane_group::Event::ZapDriveObjectInPane(uid.clone()));
            }
            Event::OpenSuggestedAgentModeWorkflowModal { workflow_and_id } => {
                ctx.emit(pane_group::Event::OpenSuggestedAgentModeWorkflowModal {
                    workflow_and_id: workflow_and_id.clone(),
                });
            }
            Event::OpenSuggestedRuleDialog { rule_and_id } => {
                ctx.emit(pane_group::Event::OpenSuggestedRuleModal {
                    rule_and_id: rule_and_id.clone(),
                });
            }
            Event::OpenAIFactCollection { sync_id } => {
                ctx.emit(pane_group::Event::OpenAIFactCollection { sync_id: *sync_id });
            }
            Event::SummarizationCancelDialogToggled { is_open } => {
                group.terminal_with_open_summarization_dialog = is_open.then_some(terminal_pane_id);
                ctx.notify();
            }
            // Zap Wave 7-3: the `Event::EnvironmentSetupModeSelectorToggled`
            // handler was physically removed along with the ambient-agent UI subsystem.
            #[cfg(feature = "local_fs")]
            Event::OpenFileWithTarget {
                path,
                target,
                line_col,
            } => {
                ctx.emit(pane_group::Event::OpenFileWithTarget {
                    path: path.clone(),
                    target: target.clone(),
                    line_col: *line_col,
                });
            }
            // Zap: forwards the terminal's "open remote file" event to pane_group → workspace.
            #[cfg(all(feature = "local_tty", feature = "local_fs"))]
            Event::OpenRemoteFileFromTerminal {
                remote_path,
                line_col,
            } => {
                ctx.emit(pane_group::Event::OpenRemoteFileFromTerminal {
                    remote_path: remote_path.clone(),
                    line_col: *line_col,
                });
            }
            Event::CopyFileToRemote { command, upload_id } => {
                let new_pane_id = group.insert_terminal_pane(
                    Direction::Right,
                    pane_id,
                    None, /*chosen_shell*/
                    ctx,
                );

                group.hide_pane_for_job(new_pane_id.into(), ctx);

                let new_terminal_view = group
                    .active_session_view(ctx)
                    .expect("should have new terminal view");
                new_terminal_view.update(ctx, |terminal_view, ctx| {
                    terminal_view.set_pending_command(command, ctx);
                    terminal_view.set_is_ssh_uploader(true);
                });

                ctx.emit(pane_group::Event::FileUploadCommand {
                    upload_id: *upload_id,
                    command: command.to_owned(),
                    remote_pane_id: terminal_pane_id,
                    local_pane_id: new_pane_id,
                });

                group.focus_pane(pane_id, true, ctx);
            }
            Event::FileUploadPasswordPending => {
                ctx.emit(pane_group::Event::FileUploadPasswordPending {
                    local_pane_id: terminal_pane_id,
                });
            }
            Event::OpenConversationHistory => {
                ctx.emit(OpenConversationHistory);
            }
            Event::FileUploadFinished(exit_code) => {
                ctx.emit(pane_group::Event::FileUploadFinished {
                    local_pane_id: terminal_pane_id,
                    exit_code: *exit_code,
                });

                // Each upload spawns its own new terminal pane. Once an upload
                // has finished, we know that its terminal session will no
                // longer be responsible for any UI-based uploads.
                if let Some(uploader_terminal_view) =
                    group.terminal_view_from_pane_id(terminal_pane_id, ctx)
                {
                    uploader_terminal_view.update(ctx, |terminal_view, _ctx| {
                        terminal_view.set_is_ssh_uploader(false);
                    });
                }
            }
            Event::OpenFileUploadSession(upload_id) => {
                ctx.emit(pane_group::Event::OpenFileUploadSession {
                    remote_pane_id: terminal_pane_id,
                    upload_id: *upload_id,
                })
            }
            Event::TerminateFileUploadSession(upload_id) => {
                ctx.emit(pane_group::Event::TerminateFileUploadSession {
                    remote_pane_id: terminal_pane_id,
                    upload_id: *upload_id,
                })
            }
            Event::OpenThemeChooser => {
                ctx.emit(pane_group::Event::OpenThemeChooser);
            }
            Event::OpenMCPSettingsPage { page } => {
                ctx.emit(pane_group::Event::OpenMCPSettingsPage { page: *page });
            }
            Event::OpenFilesPalette { source } => {
                ctx.emit(pane_group::Event::OpenFilesPalette { source: *source })
            }
            Event::OpenAddRulePane => {
                ctx.emit(crate::pane_group::Event::OpenAddRulePane);
            }
            Event::OpenRulesPane => {
                ctx.emit(crate::pane_group::Event::OpenAIFactCollection { sync_id: None });
            }
            Event::OpenAddPromptPane { initial_content } => {
                ctx.emit(crate::pane_group::Event::OpenAddPromptPane {
                    initial_content: initial_content.clone(),
                });
            }
            // Zap Wave 7-3: `OpenEnvironmentManagementPane` event forwarding was
            // physically removed along with the ambient-agent UI subsystem.
            #[cfg(feature = "local_fs")]
            Event::FileRenamed { old_path, new_path } => {
                ctx.emit(pane_group::Event::FileRenamed {
                    old_path: old_path.clone(),
                    new_path: new_path.clone(),
                });
            }
            #[cfg(feature = "local_fs")]
            Event::FileDeleted { path } => {
                ctx.emit(pane_group::Event::FileDeleted { path: path.clone() });
            }
            Event::ToggleLeftPanel {
                target_view,
                force_open,
            } => {
                ctx.emit(pane_group::Event::ToggleLeftPanel {
                    target_view: *target_view,
                    force_open: *force_open,
                });
            }
            Event::ToggleAIDocumentPane {
                document_id,
                document_version,
            } => {
                if let Some(conversation_id) =
                    crate::ai::document::ai_document_model::AIDocumentModel::as_ref(ctx)
                        .get_conversation_id_for_document_id(document_id)
                {
                    group.toggle_ai_document_pane(
                        conversation_id,
                        *document_id,
                        *document_version,
                        ctx,
                    );
                }
            }
            Event::HideAIDocumentPanes => {
                group.close_all_ai_document_panes(ctx);
            }
            Event::OpenAIDocumentPane {
                document_id,
                document_version,
                is_auto_open,
            } => {
                let should_open = if *is_auto_open {
                    // Auto-open: only open if there's already a visible plan pane
                    // (to replace it with the newest plan) or if there's enough space.
                    let has_visible_ai_doc_pane = group
                        .ai_document_panes()
                        .any(|pane_id| !group.is_pane_hidden_for_close(pane_id));

                    has_visible_ai_doc_pane
                        || group
                            .terminal_view_from_pane_id(terminal_pane_id, ctx)
                            .is_some_and(|tv| tv.as_ref(ctx).can_auto_open_panel())
                } else {
                    // User-triggered: always open.
                    true
                };

                if should_open {
                    if let Some(conversation_id) =
                        crate::ai::document::ai_document_model::AIDocumentModel::as_ref(ctx)
                            .get_conversation_id_for_document_id(document_id)
                    {
                        group.open_ai_document_pane(
                            conversation_id,
                            *document_id,
                            *document_version,
                            ctx,
                        );
                    }
                }
            }
            Event::OpenAgentProfileEditor { profile_id } => {
                ctx.emit(pane_group::Event::OpenAgentProfileEditor {
                    profile_id: *profile_id,
                });
            }
            Event::InsertCodeReviewComments {
                repo_path,
                comments,
                diff_mode,
                open_code_review,
            } => {
                ctx.emit(pane_group::Event::InsertCodeReviewComments {
                    repo_path: repo_path.to_path_buf(),
                    comments: comments.to_owned(),
                    diff_mode: diff_mode.to_owned(),
                    open_code_review: open_code_review.clone(),
                });
            }
            Event::RevealChildAgent { conversation_id } => {
                // Materialize the child pane first if the parent was restored
                // from SQLite and its children have not been rebuilt yet --
                // otherwise the pill is a dead click after every restart.
                group.ensure_hidden_child_agent_pane_for_conversation(*conversation_id, ctx);
                if let Some(&child_pane_id) = group.child_agent_panes.get(conversation_id) {
                    group.panes.show_pane_for_child_agent(child_pane_id);
                    group.handle_pane_count_change(ctx);
                    group.focus_pane(child_pane_id, true, ctx);
                } else {
                    log::warn!("No hidden pane found for child conversation {conversation_id:?}");
                }
            }
            Event::SpawnLocalChildAgents {
                parent_conversation_id,
                argument,
            } => {
                spawn_local_child_agents(group, pane_id, *parent_conversation_id, argument, ctx);
            }
            Event::OpenChildAgentInNewPane { conversation_id } => {
                // Reveals the hidden child pane as a visible sibling rather
                // than a genuinely new pane (see
                // `TerminalAction::OpenChildAgentInNewPane`'s doc comment and
                // #304's pill-bar Step 2), but it now materializes the pane on
                // demand first, so a child restored from SQLite is reachable
                // without waiting for the next cold start.
                group.ensure_hidden_child_agent_pane_for_conversation(*conversation_id, ctx);
                if let Some(&child_pane_id) = group.child_agent_panes.get(conversation_id) {
                    group.panes.show_pane_for_child_agent(child_pane_id);
                    group.handle_pane_count_change(ctx);
                    group.focus_pane(child_pane_id, true, ctx);
                } else {
                    log::warn!(
                        "OpenChildAgentInNewPane: could not materialize a hidden pane for \
                         child conversation {conversation_id:?}"
                    );
                }
            }
            Event::OpenChildAgentInNewTab { conversation_id } => {
                // Degraded, same as OpenChildAgentInNewPane above: reveals the
                // hidden pane as a sibling pane rather than a real new tab. See
                // `TerminalAction::OpenChildAgentInNewTab`'s doc comment.
                group.ensure_hidden_child_agent_pane_for_conversation(*conversation_id, ctx);
                if let Some(&child_pane_id) = group.child_agent_panes.get(conversation_id) {
                    group.panes.show_pane_for_child_agent(child_pane_id);
                    group.handle_pane_count_change(ctx);
                    group.focus_pane(child_pane_id, true, ctx);
                } else {
                    log::warn!(
                        "OpenChildAgentInNewTab: no hidden pane for child conversation \
                         {conversation_id:?}"
                    );
                }
            }
            Event::SwapPaneToConversation { conversation_id } => {
                // Swap visibility instead of cloning so in-flight state in the
                // target pane is preserved -- but the target has to exist
                // first, which after a restart means materializing it.
                if group.ensure_hidden_child_agent_pane_for_conversation(*conversation_id, ctx) {
                    group.swap_active_pane_to_conversation(pane_id, *conversation_id, ctx);
                } else {
                    log::warn!(
                        "SwapPaneToConversation: failed to materialize conversation \
                         {conversation_id:?}"
                    );
                }
            }
            Event::StopAgentConversation { conversation_id } => {
                stop_agent_conversation(group, *conversation_id, ctx);
            }
            Event::KillAgentConversation { conversation_id } => {
                let source_terminal_view_id = group
                    .terminal_view_from_pane_id(terminal_pane_id, ctx)
                    .map(|terminal_view| terminal_view.id());
                kill_agent_conversation(group, source_terminal_view_id, *conversation_id, ctx);
            }
            _ => {}
        }
    } else {
        log::warn!("Session {terminal_pane_id:?} not found");
    }
}

/// Minimal local action-state gate for the orchestration pill bar's
/// Stop/Kill actions -- a scoped-down version of the pin's
/// `AgentConversationActionState`. The pin's `task_id` /
/// `is_cloud_cancel_candidate` fields aren't carried: this fork routes
/// every stop/cancel through `TerminalView::stop_local_agent_conversation`
/// (see `stop_agent_conversation` below) rather than branching to an
/// ambient-task cloud-cancel path
/// (`crate::ai::ambient_agents::cancel_task_with_toast`/`cancel_task_silently`),
/// which doesn't exist in this fork. `is_remote_child` is also permanently
/// false here (no remote-worker execution path), so that half of the pin's
/// branch condition could never fire anyway.
#[derive(Clone, Copy)]
struct AgentConversationActionState {
    owner_terminal_view_id: EntityId,
    is_in_progress: bool,
}

fn agent_conversation_action_state(
    conversation_id: AIConversationId,
    ctx: &AppContext,
) -> Option<AgentConversationActionState> {
    let history_model = BlocklistAIHistoryModel::as_ref(ctx);
    let conversation = history_model.conversation(&conversation_id)?;
    let owner_terminal_view_id =
        history_model.terminal_view_id_for_conversation(&conversation_id)?;
    Some(AgentConversationActionState {
        owner_terminal_view_id,
        is_in_progress: conversation.status().is_in_progress(),
    })
}

/// Cross-workspace lookup for the `TerminalView` owning `owner_terminal_view_id`,
/// for when it isn't hosted in the pane group already at hand.
fn terminal_view_handle_for_owner(
    owner_terminal_view_id: EntityId,
    ctx: &AppContext,
) -> Option<ViewHandle<TerminalView>> {
    WorkspaceRegistry::as_ref(ctx)
        .all_workspaces(ctx)
        .into_iter()
        .find_map(|(_, workspace)| {
            workspace.as_ref(ctx).tab_views().find_map(|pane_group| {
                let group = pane_group.as_ref(ctx);
                let pane_id = group.find_pane_id_for_terminal_view(owner_terminal_view_id, ctx)?;
                group.terminal_view_from_pane_id(pane_id, ctx)
            })
        })
}

fn stop_agent_conversation(
    group: &PaneGroup,
    conversation_id: AIConversationId,
    ctx: &mut ViewContext<PaneGroup>,
) {
    let Some(state) = agent_conversation_action_state(conversation_id, ctx) else {
        log::warn!("StopAgentConversation: conversation {conversation_id:?} not found");
        return;
    };
    if !state.is_in_progress {
        return;
    }
    let terminal_view = group
        .find_pane_id_for_terminal_view(state.owner_terminal_view_id, ctx)
        .and_then(|pane_id| group.terminal_view_from_pane_id(pane_id, ctx))
        .or_else(|| terminal_view_handle_for_owner(state.owner_terminal_view_id, ctx));
    let Some(terminal_view) = terminal_view else {
        log::warn!(
            "StopAgentConversation: no terminal view found for conversation {conversation_id:?}"
        );
        // Still make the stop visible in history even though nothing in
        // memory could actually cancel the in-flight work, mirroring the
        // pin's fallback for a gone owner view.
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
            history_model.update_conversation_status(
                state.owner_terminal_view_id,
                conversation_id,
                crate::ai::agent::conversation::ConversationStatus::Cancelled,
                ctx,
            );
        });
        return;
    };
    terminal_view.update(ctx, |terminal_view, ctx| {
        terminal_view.stop_local_agent_conversation(conversation_id, ctx);
    });
}

fn kill_agent_conversation(
    group: &mut PaneGroup,
    source_terminal_view_id: Option<EntityId>,
    conversation_id: AIConversationId,
    ctx: &mut ViewContext<PaneGroup>,
) {
    let state = agent_conversation_action_state(conversation_id, ctx);

    // Tombstone before anything else so a late local event for this
    // conversation can't recreate it. Local equivalent of the pin's
    // `OrchestrationEventStreamer::mark_conversation_killed` -- that
    // streamer is cloud (DECLINED.md, orchestration_event_streamer.rs) and
    // not ported. See `BlocklistAIHistoryModel::mark_conversation_killed`'s
    // doc comment: this guards the one local re-creation path this fork
    // was confirmed to have (`start_new_child_conversation`); it is not a
    // proven-exhaustive port of the pin's event-batch-level filtering,
    // which guards a server-relay path this fork doesn't have at all.
    BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, _| {
        history_model.mark_conversation_killed(conversation_id);
    });

    if let Some(state) = state
        && state.is_in_progress
    {
        let terminal_view = group
            .find_pane_id_for_terminal_view(state.owner_terminal_view_id, ctx)
            .and_then(|pane_id| group.terminal_view_from_pane_id(pane_id, ctx))
            .or_else(|| terminal_view_handle_for_owner(state.owner_terminal_view_id, ctx));
        if let Some(terminal_view) = terminal_view {
            terminal_view.update(ctx, |terminal_view, ctx| {
                terminal_view.stop_local_agent_conversation(conversation_id, ctx);
            });
        }
    }

    let owner_terminal_view_id = state
        .map(|state| state.owner_terminal_view_id)
        .or(source_terminal_view_id);

    if !group.discard_child_agent_pane_for_conversation(conversation_id, ctx) {
        log::warn!("KillAgentConversation: no child pane found for {conversation_id:?}");
    }

    if owner_terminal_view_id.is_none() {
        log::warn!(
            "KillAgentConversation: no terminal view found for conversation {conversation_id:?}"
        );
    }
    // Delete (not remove): drop the conversation from sqlite so a killed
    // child does not resurrect on restart. The pin also drops a cloud copy
    // here; there is no cloud copy in this fork.
    crate::ai::conversation_utils::delete_conversation(
        conversation_id,
        owner_terminal_view_id,
        ctx,
    );
}

/// Handles `TerminalAction::SpawnLocalChildAgents` (the `/orchestrate` slash
/// command): prepares one local child harness launch per task and, once
/// each is ready, materializes it via `finish_spawning_local_child_agent`.
/// User-invoked only -- see `TerminalAction::SpawnLocalChildAgents`'s doc
/// comment.
fn spawn_local_child_agents(
    group: &mut PaneGroup,
    base_pane_id: PaneId,
    parent_conversation_id: AIConversationId,
    argument: &str,
    ctx: &mut ViewContext<PaneGroup>,
) {
    let tasks = super::local_harness_launch::split_orchestrate_tasks(argument);
    if tasks.is_empty() {
        log::warn!("SpawnLocalChildAgents: no tasks parsed from {argument:?}");
        return;
    }

    let Some(base_terminal_view) = group.terminal_view_from_pane_id(base_pane_id, ctx) else {
        log::warn!("SpawnLocalChildAgents: no terminal view for pane {base_pane_id:?}");
        return;
    };
    // Structural context inheritance: same shell and working directory as
    // the pane `/orchestrate` was typed in. See `compose_child_agent_prompt`
    // for why the prompt text itself carries nothing beyond the task.
    let shell_type = base_terminal_view
        .as_ref(ctx)
        .active_session_shell_type(ctx);
    let startup_directory =
        group.startup_path_for_new_session(base_pane_id.as_terminal_pane_id(), ctx);
    let parent_run_id = BlocklistAIHistoryModel::as_ref(ctx)
        .conversation(&parent_conversation_id)
        .and_then(|conversation| conversation.agent_link_id());

    for task in tasks {
        let prompt = super::local_harness_launch::compose_child_agent_prompt(&task);
        if prompt.is_empty() {
            continue;
        }
        let agent_name = prompt.clone();
        let future = super::local_harness_launch::prepare_local_harness_child_launch(
            prompt,
            super::local_harness_launch::ORCHESTRATE_DEFAULT_HARNESS.to_string(),
            // /orchestrate has no model-selection flag (see
            // `ORCHESTRATE_DEFAULT_HARNESS`'s doc comment above), so there is
            // never a per-child model override here.
            None,
            parent_run_id.clone(),
            shell_type,
            startup_directory.clone(),
        );
        let _ = ctx.spawn(future, move |group, result, ctx| match result {
            Ok(prepared) => {
                finish_spawning_local_child_agent(
                    group,
                    base_pane_id,
                    parent_conversation_id,
                    agent_name,
                    prepared,
                    ctx,
                );
            }
            Err(message) => {
                log::error!(
                    "SpawnLocalChildAgents: failed to prepare local child launch: {message}"
                );
                ctx.emit(pane_group::Event::ShowToast {
                    message: format!("Could not start child agent: {message}"),
                    flavor: ToastFlavor::Error,
                    pane_id: Some(base_pane_id),
                });
            }
        });
    }
}

/// Materializes one spawned child agent: creates its hidden pane (inheriting
/// the prepared env vars, notably `OZ_RUN_ID`/`OZ_PARENT_RUN_ID`), registers
/// its conversation in the orchestration topology
/// (`BlocklistAIHistoryModel::children_by_parent`, `PaneGroup::child_agent_panes`),
/// and types the harness command into its PTY.
///
/// This is the actual display-path wiring #325 asked for: once this
/// returns, the orchestration pill bar, `ChildAgentStatusCard`, and
/// transcript rendering (all landed earlier on this branch, #304) pick the
/// child up through the same `BlocklistAIHistoryModel`/`PaneGroup` state
/// they already render restored children from -- nothing downstream needs
/// to know a child was spawned by `/orchestrate` specifically.
fn finish_spawning_local_child_agent(
    group: &mut PaneGroup,
    base_pane_id: PaneId,
    parent_conversation_id: AIConversationId,
    agent_name: String,
    prepared: super::local_harness_launch::PreparedLocalHarnessLaunch,
    ctx: &mut ViewContext<PaneGroup>,
) {
    let new_pane_id =
        group.insert_terminal_pane_hidden_for_child_agent(base_pane_id, prepared.env_vars, ctx);
    let Some(new_terminal_view) = group.terminal_view_from_pane_id(new_pane_id, ctx) else {
        log::error!("SpawnLocalChildAgents: failed to get terminal view for spawned child pane");
        group.discard_pane(new_pane_id.into(), ctx);
        return;
    };
    let child_terminal_view_id = new_terminal_view.id();

    // The child's own view id is its `terminal_view_id`, matching how a
    // brand-new top-level conversation is always registered under its own
    // view (`enter_agent_view_internal` uses `self.terminal_view_id`) --
    // not the parent's. `enter_agent_view` below checks liveness via
    // `all_live_conversations_for_terminal_view(self.view_id)`, so
    // registering under any other view would make that check fail.
    let child_id = BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
        let child_id = history_model.start_new_child_conversation(
            child_terminal_view_id,
            agent_name,
            parent_conversation_id,
            Some(Harness::Claude),
            ctx,
        );
        history_model.assign_run_id_for_conversation(
            child_id,
            prepared.run_id.clone(),
            Some(prepared.task_id),
            child_terminal_view_id,
            ctx,
        );
        child_id
    });

    new_terminal_view.update(ctx, |terminal_view, ctx| {
        terminal_view.enter_agent_view(None, Some(child_id), AgentViewEntryOrigin::ChildAgent, ctx);
        terminal_view.start_local_child_harness_process(&prepared.command, ctx);
    });

    group.child_agent_panes.insert(child_id, new_pane_id.into());
}

/// Exchanges whose `Pending` query row is already persisted, keyed to the part of the row that
/// can change while streaming (`inputs.len()`); everything else in the row is fixed per exchange.
#[cfg(feature = "local_fs")]
type PersistedPendingExchanges =
    std::collections::HashMap<crate::ai::agent::AIAgentExchangeId, usize>;

/// Whether an exchange event needs an `UpsertAIQuery`. `UpdatedStreamingExchange` fires once
/// per streamed delta, but the persisted row is identical for every delta of a `Pending`
/// exchange, so only the first (and any change in input count) is written. `AppendedExchange`
/// and any non-`Pending` status always write; terminal statuses drop the entry (#1137).
#[cfg(feature = "local_fs")]
fn should_upsert_ai_query(
    persisted: &mut PersistedPendingExchanges,
    exchange_id: crate::ai::agent::AIAgentExchangeId,
    status: &crate::ai::blocklist::AIQueryHistoryOutputStatus,
    input_count: usize,
    is_streaming_delta: bool,
) -> bool {
    if *status != crate::ai::blocklist::AIQueryHistoryOutputStatus::Pending {
        persisted.remove(&exchange_id);
        return true;
    }
    let unchanged = persisted.get(&exchange_id) == Some(&input_count);
    persisted.insert(exchange_id, input_count);
    !(is_streaming_delta && unchanged)
}

#[cfg(feature = "local_fs")]
fn handle_ai_history_event(
    event: &BlocklistAIHistoryEvent,
    terminal_view_id: EntityId,
    terminal_pane_id: TerminalPaneId,
    model_event_sender: SyncSender<ModelEvent>,
    is_shared_ambient_agent_session: bool,
    persisted_pending: &mut PersistedPendingExchanges,
    ctx: &mut ViewContext<PaneGroup>,
) {
    use std::sync::Arc;

    use crate::ai::blocklist::{
        AIQueryHistoryOutputStatus, PersistedAIInput, PersistedAIInputType,
    };

    if event
        .terminal_view_id()
        .is_some_and(|id| id != terminal_view_id)
    {
        return;
    }

    match event {
        BlocklistAIHistoryEvent::AppendedExchange {
            exchange_id,
            conversation_id,
            is_hidden,
            ..
        }
        | BlocklistAIHistoryEvent::UpdatedStreamingExchange {
            exchange_id,
            conversation_id,
            is_hidden,
            ..
        } => {
            // Check if session restoration is enabled.
            if !*GeneralSettings::as_ref(ctx).restore_session
                || !AppExecutionMode::as_ref(ctx).can_save_session()
            {
                return;
            }

            let Some(conversation) =
                BlocklistAIHistoryModel::as_ref(ctx).conversation(conversation_id)
            else {
                log::warn!("Received event with invalid conversation ID: {conversation_id:?}");
                return;
            };

            let Some(exchange) = conversation.exchange_with_id(*exchange_id) else {
                log::warn!("Received event with invalid exchange ID: {exchange_id:?}");
                return;
            };

            // Hidden blocks and passive-only conversations should not be restored, so we skip
            // them.
            if *is_hidden || conversation.is_entirely_passive() {
                return;
            }

            // Do not persist AI queries from shared ambient agent sessions that we've viewed,
            // as these were sent as part of an ambient agent run and shouldn't pollute the up arrow history.
            if is_shared_ambient_agent_session {
                return;
            }

            let inputs: Vec<_> = exchange
                .input
                .iter()
                .filter_map(|input| PersistedAIInputType::try_from(input).ok())
                .collect();
            let output_status = AIQueryHistoryOutputStatus::from(&exchange.output_status);
            if !should_upsert_ai_query(
                persisted_pending,
                exchange.id,
                &output_status,
                inputs.len(),
                matches!(
                    event,
                    BlocklistAIHistoryEvent::UpdatedStreamingExchange { .. }
                ),
            ) {
                return;
            }

            let persisted_query = PersistedAIInput {
                start_ts: exchange.start_time,
                inputs,
                exchange_id: exchange.id,
                conversation_id: *conversation_id,
                output_status,
                working_directory: exchange.working_directory.clone(),
                // TODO(CORE-3546): shell: exchange.shell.clone(),
                model_id: exchange.model_id.clone(),
                coding_model_id: exchange.coding_model_id.clone(),
            };
            let upsert_ai_query_event = ModelEvent::UpsertAIQuery {
                query: Arc::new(persisted_query),
            };
            let _ = ctx.spawn(
                // Sending over a sync sender can block the current thread, so we
                // do this async.
                async move { model_event_sender.send(upsert_ai_query_event) },
                move |_, res, _| {
                    if let Err(err) = res {
                        log::error!(
                            "Error sending upsert AI query event for terminal id {terminal_pane_id:?} {err:?}"
                        );
                    }
                },
            );
        }
        BlocklistAIHistoryEvent::ClearedConversationsInTerminalView { .. }
        | BlocklistAIHistoryEvent::ClearedActiveConversation { .. } => {
            ctx.emit(pane_group::Event::InvalidatedActiveConversation);
        }
        BlocklistAIHistoryEvent::RemoveConversation {
            conversation_id, ..
        } => {
            let conversation_id = conversation_id.to_string();
            // On remove, delete all related AI query and multi-agent conversation data for this conversation.
            let _ = ctx.spawn(
                async move {
                    model_event_sender.send(ModelEvent::DeleteAIConversation {
                        conversation_id: conversation_id.clone(),
                    })?;
                    model_event_sender.send(ModelEvent::DeleteMultiAgentConversations {
                        conversation_ids: vec![conversation_id],
                    })
                },
                |_, res, _| {
                    if let Err(err) = res {
                        log::error!("Error sending delete events for conversation: {err:?}");
                    }
                },
            );
        }
        // DeletedConversation SQL cleanup is handled directly in delete_conversation().
        BlocklistAIHistoryEvent::DeletedConversation { .. }
        | BlocklistAIHistoryEvent::StartedNewConversation { .. }
        | BlocklistAIHistoryEvent::UpdatedConversationStatus { .. }
        | BlocklistAIHistoryEvent::ReassignedExchange { .. }
        | BlocklistAIHistoryEvent::SetActiveConversation { .. }
        | BlocklistAIHistoryEvent::UpdatedTodoList { .. }
        | BlocklistAIHistoryEvent::UpdatedAutoexecuteOverride { .. }
        | BlocklistAIHistoryEvent::SplitConversation { .. }
        | BlocklistAIHistoryEvent::RestoredConversations { .. }
        | BlocklistAIHistoryEvent::CreatedSubtask { .. }
        | BlocklistAIHistoryEvent::UpgradedTask { .. }
        | BlocklistAIHistoryEvent::UpdatedConversationMetadata { .. }
        | BlocklistAIHistoryEvent::UpdatedConversationTitle { .. }
        | BlocklistAIHistoryEvent::UpdatedConversationArtifacts { .. }
        | BlocklistAIHistoryEvent::ConversationAgentIdAssigned { .. }
        | BlocklistAIHistoryEvent::ConversationTransferredBetweenTerminalViews { .. } => (),
    }
}

#[cfg(test)]
#[path = "terminal_pane_tests.rs"]
mod tests;

/// Whether detaching a terminal pane ends its session at once instead of keeping it for the
/// undo-close grace period (#1001): a reversible close (`HiddenForClose`) of a remote session
/// that never bootstrapped. A move keeps everything running; a permanent close already ends it.
fn detach_type_skips_undo_grace(detach_type: DetachType, unbootstrapped_remote: bool) -> bool {
    unbootstrapped_remote && matches!(detach_type, DetachType::HiddenForClose)
}

/// Why the pane of a shell that just exited stays open, before the pane's own checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitedShellKeep {
    NotBootstrapped,
    SessionEnd,
    No,
}

/// `session_end_gate` is the Windows gate's verdict, which consumes one counted session-end exit
/// per call. It is asked for every exit, a shell that never bootstrapped included: an exit left
/// unconsumed would make a later ordinary exit (the user typed `exit`) keep its pane as if
/// Windows had killed it (#1617).
fn exited_shell_keeps_pane(
    bootstrapped: bool,
    session_end_gate: impl FnOnce() -> bool,
) -> ExitedShellKeep {
    let session_end = session_end_gate();
    if !bootstrapped {
        ExitedShellKeep::NotBootstrapped
    } else if session_end {
        ExitedShellKeep::SessionEnd
    } else {
        ExitedShellKeep::No
    }
}

#[cfg(test)]
mod exited_shell_tests {
    use std::cell::Cell;

    use super::*;

    /// The gate's count, as `SessionEndGate::should_keep_exited_pane` consumes it after a
    /// cancelled session end: one kept pane per counted exit.
    fn cancelled_gate(counted: &Cell<u32>) -> impl FnOnce() -> bool + '_ {
        move || {
            let had = counted.get();
            counted.set(had.saturating_sub(1));
            had > 0
        }
    }

    #[test]
    fn an_unbootstrapped_exit_consumes_its_session_end_count() {
        // One shell killed by a queried-then-cancelled session end; it never bootstrapped.
        let counted = Cell::new(1);
        assert_eq!(
            exited_shell_keeps_pane(false, cancelled_gate(&counted)),
            ExitedShellKeep::NotBootstrapped
        );
        assert_eq!(counted.get(), 0);
        // The user later types `exit` in another pane: it closes.
        assert_eq!(
            exited_shell_keeps_pane(true, cancelled_gate(&counted)),
            ExitedShellKeep::No
        );
    }

    #[test]
    fn a_counted_exit_of_a_bootstrapped_shell_keeps_its_pane_once() {
        let counted = Cell::new(1);
        assert_eq!(
            exited_shell_keeps_pane(true, cancelled_gate(&counted)),
            ExitedShellKeep::SessionEnd
        );
        assert_eq!(
            exited_shell_keeps_pane(true, cancelled_gate(&counted)),
            ExitedShellKeep::No
        );
    }
}

#[cfg(test)]
mod undo_grace_tests {
    use super::*;

    #[test]
    fn only_a_reversible_close_of_an_unbootstrapped_remote_skips_the_grace() {
        assert!(detach_type_skips_undo_grace(
            DetachType::HiddenForClose,
            true
        ));
        assert!(!detach_type_skips_undo_grace(
            DetachType::HiddenForClose,
            false
        ));
        assert!(!detach_type_skips_undo_grace(DetachType::Moved, true));
        assert!(!detach_type_skips_undo_grace(DetachType::Closed, true));
    }
}
