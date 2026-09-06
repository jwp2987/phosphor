//! Implementation of terminal panes.
#[cfg(feature = "local_fs")]
use crate::pane_group::CodeSource;
use std::sync::mpsc::SyncSender;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use warp_multi_agent_api as multi_agent_api;

use warpui::{
    AppContext, EntityId, ModelHandle, SingletonEntity, ViewContext, ViewHandle, WindowId,
};

use crate::{
    ai::{
        agent::conversation::AIConversationId,
        blocklist::{agent_view::AgentViewControllerEvent, BlocklistAIHistoryModel},
        llms::LLMPreferences,
        skills::SkillManager,
    },
    app_state::{AmbientAgentPaneSnapshot, LeafContents, TerminalPaneSnapshot},
    pane_group::{self, Direction, Event::OpenConversationHistory, PaneGroup},
    persistence::{BlockCompleted, ModelEvent},
    session_management::SessionNavigationData,
    terminal::cli_agent_sessions::CLIAgentSessionsModel,
    terminal::{
        general_settings::GeneralSettings, shared_session::SharedSessionStatus, view::Event,
        TerminalManager, TerminalView,
    },
    view_components::ToastFlavor,
    workspace::{sync_inputs::SyncedInputState, PaneViewLocator, WorkspaceRegistry},
    AIExecutionProfilesModel,
};

#[cfg(feature = "local_fs")]
use crate::ai::blocklist::BlocklistAIHistoryEvent;

#[cfg(not(target_family = "wasm"))]
use crate::ai::blocklist::agent_view::AgentViewEntryOrigin;
#[cfg(not(target_family = "wasm"))]
use warp_cli::agent::Harness;

use warp_core::execution_mode::AppExecutionMode;

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
}

fn resolve_runtime_skills(
    skill_references: &[ai::skills::SkillReference],
    ctx: &AppContext,
) -> Result<Vec<String>, Vec<String>> {
    let skill_manager = SkillManager::as_ref(ctx);
    let mut runtime_skills = Vec::with_capacity(skill_references.len());
    let mut unresolved_references = Vec::new();

    for reference in skill_references {
        let Some(skill) = skill_manager.skill_by_reference(reference) else {
            unresolved_references.push(reference.to_string());
            continue;
        };
        runtime_skills.push(serialize_proto_to_base64(&multi_agent_api::Skill::from(
            skill.clone(),
        )));
    }

    if unresolved_references.is_empty() {
        Ok(runtime_skills)
    } else {
        Err(unresolved_references)
    }
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
        }
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
        if matches!(detach_type, DetachType::Closed) {
            // Only immediately clear conversations and delete blocks if the session is being
            // permanently closed.
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
                history_model
                    .clear_conversations_in_terminal_view(self.terminal_view(ctx).id(), ctx);
            });
            self.delete_blocks(ctx);
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
                is_conversation_only: false,
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
                    is_conversation_only: false,
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

            LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: self.uuid.clone(),
                cwd: view.pwd_if_local(app),
                is_active,
                is_read_only: view.model.lock().is_read_only(),
                shell_launch_data: view.shell_launch_data_if_local(app),
                input_config: Some(current_input_config),
                llm_model_override,
                active_profile_id,
                conversation_ids_to_restore,
                active_conversation_id,
                is_conversation_only: view.is_conversation_pane(),
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
                // If the shell process exited before it successfully bootstrapped,
                // keep the pane open.  There might be useful information visible
                // in the output, and if this was the first shell spawned when the
                // user started the app, it will prevent it from suddenly quitting.
                if group
                    .terminal_view_from_pane_id(terminal_pane_id, ctx)
                    .is_some_and(|terminal_view| {
                        !terminal_view.as_ref(ctx).is_login_shell_bootstrapped()
                    })
                {
                    return;
                }

                group.close_pane(pane_id, ctx);
            }
            Event::CloseRequested => {
                group.close_pane_with_confirmation(pane_id, ctx);
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
            Event::BlockCompleted { block, is_local } => {
                match group.terminal_session_by_id(pane_id) {
                    Some(pane) => {
                        if *GeneralSettings::as_ref(ctx).restore_session
                            && AppExecutionMode::as_ref(ctx).can_save_session()
                        {
                            if let Some(sender) = &group.model_event_sender {
                                let block_completed_event = ModelEvent::SaveBlock(BlockCompleted {
                                    pane_id: pane.session_uuid(),
                                    block: block.clone(),
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
            #[cfg(not(target_family = "wasm"))]
            Event::OpenPluginInstructionsPane(agent, kind) => {
                ctx.emit(pane_group::Event::OpenPluginInstructionsPane(*agent, *kind));
            }
            Event::AskAIAssistant(ask_type) => {
                ctx.emit(pane_group::Event::AskAIAssistant(ask_type.to_owned()))
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
                #[cfg(not(target_family = "wasm"))]
                spawn_local_child_agents(group, pane_id, *parent_conversation_id, argument, ctx);
                #[cfg(target_family = "wasm")]
                {
                    let _ = (parent_conversation_id, argument);
                    log::warn!("SpawnLocalChildAgents is not supported on wasm");
                }
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
#[cfg(not(target_family = "wasm"))]
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
    let shell_type = base_terminal_view.as_ref(ctx).active_session_shell_type(ctx);
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
#[cfg(not(target_family = "wasm"))]
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

#[cfg(feature = "local_fs")]
fn handle_ai_history_event(
    event: &BlocklistAIHistoryEvent,
    terminal_view_id: EntityId,
    terminal_pane_id: TerminalPaneId,
    model_event_sender: SyncSender<ModelEvent>,
    is_shared_ambient_agent_session: bool,
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

            let persisted_query = PersistedAIInput {
                start_ts: exchange.start_time,
                inputs: exchange
                    .input
                    .iter()
                    .filter_map(|input| PersistedAIInputType::try_from(input).ok())
                    .collect(),
                exchange_id: exchange.id,
                conversation_id: *conversation_id,
                output_status: AIQueryHistoryOutputStatus::from(&exchange.output_status),
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

#[cfg(all(test, not(target_family = "wasm")))]
#[path = "terminal_pane_tests.rs"]
mod tests;
