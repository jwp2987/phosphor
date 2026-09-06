use std::rc::Rc;

use crate::terminal::shared_session::protocol::SessionSourceType;
use warp_core::settings::Setting as _;
use warpui::{App, AppContext, SingletonEntity, ViewContext};

use crate::{
    ai::{
        agent::{
            conversation::AIConversationId, task::TaskId, AIAgentInput, ServerOutputId,
            UserQueryMode,
        },
        blocklist::{
            agent_view::AgentViewEntryOrigin,
            block::cli_controller::UserTakeOverReason,
            model::{AIBlockModel, AIBlockOutputStatus, AIRequestType, OutputStatusUpdateCallback},
            AIBlock, ClientIdentifiers,
        },
        llms::LLMId,
    },
    features::FeatureFlag,
    settings::AISettings,
    terminal::cli_agent_sessions::{
        CLIAgentInputState, CLIAgentSession, CLIAgentSessionContext, CLIAgentSessionStatus,
        CLIAgentSessionsModel,
    },
    terminal::model::ansi::{BootstrappedValue, Handler as _, InitShellValue},
    terminal::CLIAgent,
    test_util::{add_window_with_terminal, terminal::initialize_app_for_terminal_view},
};

// `RichContentInsertionPosition` used to arrive via `use super::*`, because
// `mod.rs` imported it for the block-list insertion that the window footer bar
// replaced. `insert_pending_ai_block` below still needs it, so it is imported
// here directly rather than kept alive in `mod.rs` as an otherwise-unused import.
use super::super::{
    AIBlockMetadata, RichContentInsertionPosition, RichContentMetadata, RichContentType,
};
use super::*;

#[test]
fn deepseek_uses_bracketed_paste_submission() {
    assert_eq!(
        rich_input_submit_strategy(CLIAgent::DeepSeek),
        RichInputSubmitStrategy::BracketedPaste
    );
}

/// Ported from warp/master's `test_rich_input_submit_strategy_for_oh_my_pi`. Zap's `CLIAgent`
/// names this variant `Omp` (its command prefix), not `OhMyPi`.
#[test]
fn omp_uses_bracketed_paste_submission() {
    assert_eq!(
        rich_input_submit_strategy(CLIAgent::Omp),
        RichInputSubmitStrategy::BracketedPaste
    );
}

/// Hermes interprets embedded newlines as submit actions when text is written
/// directly. Bracketed paste preserves them as part of one input payload.
#[test]
fn test_rich_input_submit_strategy_for_hermes_uses_bracketed_paste() {
    assert_eq!(
        rich_input_submit_strategy(CLIAgent::Hermes),
        RichInputSubmitStrategy::BracketedPaste
    );
}

struct PendingAIBlockModel {
    conversation_id: AIConversationId,
    input: Vec<AIAgentInput>,
    model_id: LLMId,
}

impl PendingAIBlockModel {
    fn new(conversation_id: AIConversationId, input: Vec<AIAgentInput>) -> Self {
        Self {
            conversation_id,
            input,
            model_id: LLMId::from("fake-llm"),
        }
    }
}

impl AIBlockModel for PendingAIBlockModel {
    type View = AIBlock;

    fn status(&self, _app: &AppContext) -> AIBlockOutputStatus {
        AIBlockOutputStatus::Pending
    }

    fn server_output_id(&self, _app: &AppContext) -> Option<ServerOutputId> {
        None
    }

    fn model_id(&self, _app: &AppContext) -> Option<LLMId> {
        None
    }

    fn base_model<'a>(&'a self, _app: &'a AppContext) -> Option<&'a LLMId> {
        Some(&self.model_id)
    }

    fn inputs_to_render<'a>(&'a self, _app: &'a AppContext) -> &'a [AIAgentInput] {
        &self.input
    }

    fn conversation_id(&self, _app: &AppContext) -> Option<AIConversationId> {
        Some(self.conversation_id)
    }

    fn on_updated_output(
        &self,
        _callback: OutputStatusUpdateCallback<AIBlock>,
        _ctx: &mut ViewContext<AIBlock>,
    ) {
    }

    fn request_type(&self, _app: &AppContext) -> AIRequestType {
        AIRequestType::Active
    }
}

fn simulate_user_started_long_running_command(view: &mut TerminalView) {
    {
        let mut model = view.model.lock();
        model.init_shell(InitShellValue {
            session_id: 0.into(),
            shell: "zsh".to_owned(),
            ..Default::default()
        });
        model.bootstrapped(BootstrappedValue {
            shell: "zsh".to_owned(),
            ..Default::default()
        });
        model.simulate_long_running_block("ssh localhost", "Password:");
    }
}

fn transition_to_user_handoff_state(
    view: &mut TerminalView,
    reason: UserTakeOverReason,
    ctx: &mut ViewContext<TerminalView>,
) -> AIConversationId {
    let conversation_id = view.agent_view_controller().update(ctx, |controller, ctx| {
        controller
            .try_enter_inline_agent_view(None, AgentViewEntryOrigin::LongRunningCommand, ctx)
            .expect("inline agent view should create a conversation")
    });
    view.model
        .lock()
        .block_list_mut()
        .active_block_mut()
        .set_is_agent_tagged_in(true);

    let task_id = TaskId::new("test-task".to_owned());
    view.model
        .lock()
        .block_list_mut()
        .active_block_mut()
        .set_agent_interaction_mode_for_agent_monitored_command(&task_id, conversation_id)
        .expect("tagged-in command should transition to agent-monitored");

    view.cli_subagent_controller.update(ctx, |controller, ctx| {
        controller.switch_control_to_user(reason, ctx);
    });

    conversation_id
}

fn insert_pending_ai_block(
    view: &mut TerminalView,
    conversation_id: AIConversationId,
    ctx: &mut ViewContext<TerminalView>,
) {
    let ai_block_model = Rc::new(PendingAIBlockModel::new(
        conversation_id,
        vec![AIAgentInput::UserQuery {
            query: "help with this running command".to_owned(),
            context: vec![].into(),
            static_query_type: None,
            referenced_attachments: Default::default(),
            user_query_mode: UserQueryMode::default(),
            running_command: None,
            intended_agent: None,
        }],
    ));
    let ai_block = ctx.add_typed_action_view(|ctx| {
        AIBlock::new(
            ai_block_model.clone(),
            view.model.clone(),
            ClientIdentifiers {
                client_exchange_id: Default::default(),
                conversation_id,
                response_stream_id: None,
            },
            view.ai_controller.clone(),
            None,
            None,
            view.ai_action_model.clone(),
            view.ai_context_model.clone(),
            view.find_model.clone(),
            view.active_session.clone(),
            view.ambient_agent_view_model.clone(),
            &view.cli_subagent_controller,
            &view.model_events_handle,
            view.agent_view_controller.clone(),
            view.view_handle.clone(),
            view.id(),
            ctx,
        )
    });

    view.insert_rich_content(
        Some(RichContentType::AIBlock),
        ai_block.clone(),
        Some(RichContentMetadata::AIBlock(AIBlockMetadata {
            exchange_id: Default::default(),
            conversation_id,
            ai_block_handle: ai_block,
        })),
        RichContentInsertionPosition::Append {
            insert_below_long_running_block: false,
        },
        ctx,
    );
}

#[test]
fn use_agent_footer_renders_for_manual_handoff_even_when_user_command_footer_setting_disabled() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            let _ = settings
                .should_render_use_agent_footer_for_user_commands
                .set_value(false, ctx);
        });

        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |view, ctx| {
            simulate_user_started_long_running_command(view);

            view.refresh_use_agent_footer(ctx);
            {
                let model = view.model.lock();
                assert!(!view.should_render_use_agent_footer(&model, ctx));
                let active_block_index = model.block_list().active_block_index();
                assert!(model
                    .block_list()
                    .last_non_hidden_rich_content_block_after_block(Some(active_block_index))
                    .is_none());
                // The block-list assertion above is now vacuous -- nothing inserts the
                // footer there any more (§8 step 3) -- so assert what actually decides
                // visibility: the window footer bar's per-frame predicate.
                assert_eq!(
                    view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                    None,
                );
            }

            transition_to_user_handoff_state(view, UserTakeOverReason::Manual, ctx);

            view.refresh_use_agent_footer(ctx);
            let model = view.model.lock();
            assert!(view.should_render_use_agent_footer(&model, ctx));
            // Contract change, `docs/DESIGN-PHOSPHOR-FORK.md` §8 step 3. This used to
            // assert that the footer's view id was the last rich content after the
            // active block -- i.e. that it had been injected *into the block list*
            // underneath the running command. That injection is the defect §8 exists to
            // remove: it occupies rows the pty was told it had and cannot see. The
            // footer is now rendered by the window footer bar, outside the block list,
            // so the assertion is inverted (nothing in the block list) and the identity
            // check moves to the bar's own per-frame predicate.
            assert!(
                model
                    .block_list()
                    .rich_content_row_range(view.use_agent_footer.id())
                    .is_none(),
                "the Use Agent footer must not be injected into the block list",
            );
            assert_eq!(
                view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                Some(view.use_agent_footer.id()),
                "the window footer bar should be rendering the Use Agent footer",
            );
        });
    })
}

#[test]
fn use_agent_footer_renders_for_manual_handoff_when_unfinished_ai_block_remains() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);

        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |view, ctx| {
            simulate_user_started_long_running_command(view);

            let conversation_id = view.agent_view_controller().update(ctx, |controller, ctx| {
                controller
                    .try_enter_inline_agent_view(
                        None,
                        AgentViewEntryOrigin::LongRunningCommand,
                        ctx,
                    )
                    .expect("inline agent view should create a conversation")
            });
            view.model
                .lock()
                .block_list_mut()
                .active_block_mut()
                .set_is_agent_tagged_in(true);
            let task_id = TaskId::new("test-task".to_owned());
            view.model
                .lock()
                .block_list_mut()
                .active_block_mut()
                .set_agent_interaction_mode_for_agent_monitored_command(&task_id, conversation_id)
                .expect("tagged-in command should transition to agent-monitored");

            insert_pending_ai_block(view, conversation_id, ctx);
            assert!(view.active_ai_block(ctx).is_some());

            view.cli_subagent_controller.update(ctx, |controller, ctx| {
                controller.switch_control_to_user(UserTakeOverReason::Manual, ctx);
            });
        });

        terminal.read(&app, |view, ctx| {
            let model = view.model.lock();
            assert!(view.should_render_use_agent_footer(&model, ctx));
            // Contract change, `docs/DESIGN-PHOSPHOR-FORK.md` §8 step 3. This used to
            // assert that the footer's view id was the last rich content after the
            // active block -- i.e. that it had been injected *into the block list*
            // underneath the running command. That injection is the defect §8 exists to
            // remove: it occupies rows the pty was told it had and cannot see. The
            // footer is now rendered by the window footer bar, outside the block list,
            // so the assertion is inverted (nothing in the block list) and the identity
            // check moves to the bar's own per-frame predicate.
            assert!(
                model
                    .block_list()
                    .rich_content_row_range(view.use_agent_footer.id())
                    .is_none(),
                "the Use Agent footer must not be injected into the block list",
            );
            assert_eq!(
                view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                Some(view.use_agent_footer.id()),
                "the window footer bar should be rendering the Use Agent footer",
            );
        });
    })
}

/// During the setup phase of an ambient-agent shared session — LRCs
/// running before any CLI agent has started — the use-agent footer must stay
/// hidden.
#[test]
fn use_agent_footer_hidden_during_ambient_agent_setup_lrc() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |view, ctx| {
            simulate_user_started_long_running_command(view);

            // Ambient-agent setup phase: ambient source type set, LRC running,
            // NO CLIAgentSession registered yet.
            view.model
                .lock()
                .set_shared_session_source_type(SessionSourceType::AmbientAgent { task_id: None });
            assert!(view.model.lock().is_shared_ambient_agent_session());
            assert!(
                CLIAgentSessionsModel::as_ref(ctx)
                    .session(view.id())
                    .is_none(),
                "precondition: no CLI agent session yet",
            );

            view.refresh_use_agent_footer(ctx);

            let model = view.model.lock();
            assert!(
                !view.should_render_use_agent_footer(&model, ctx),
                "footer should be hidden during ambient-agent setup LRCs",
            );
            let active_block_index = model.block_list().active_block_index();
            assert!(
                model
                    .block_list()
                    .last_non_hidden_rich_content_block_after_block(Some(active_block_index))
                    .is_none(),
                "footer rich content should not be in the blocklist during ambient setup",
            );
            // …and, since §8 step 3, that assertion is vacuous on its own: the bar is
            // what renders the footer, so this is the check that can still fail.
            assert_eq!(
                view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                None,
                "the window footer bar must stay empty during ambient-agent setup LRCs",
            );
        });
    })
}

/// When viewing a shared ambient-agent session whose sharer is
/// running a CLI agent, the CLI agent footer should still render.
#[test]
fn cli_agent_footer_renders_for_viewer_of_shared_ambient_agent_session() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |view, ctx| {
            simulate_user_started_long_running_command(view);

            // Mark the model as a shared ambient agent session, mirroring
            // what the viewer's terminal manager does on `JoinedSuccessfully`.
            view.model
                .lock()
                .set_shared_session_source_type(SessionSourceType::AmbientAgent { task_id: None });
            assert!(view.model.lock().is_shared_ambient_agent_session());

            // Inject the CLI agent session state that an old shared-session viewer
            // would have received from the sharer.
            let view_id = view.id();
            CLIAgentSessionsModel::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.set_session(
                    view_id,
                    CLIAgentSession {
                        agent: CLIAgent::Claude,
                        status: CLIAgentSessionStatus::InProgress,
                        session_context: CLIAgentSessionContext::default(),
                        input_state: CLIAgentInputState::Closed,
                        listener: None,
                        plugin_version: None,
                        remote_host: None,
                        draft_text: None,
                        custom_command_prefix: None,
                        should_auto_toggle_input: false,
                        received_rich_notification: false,
                    },
                    ctx,
                );
            });

            view.refresh_use_agent_footer(ctx);

            let model = view.model.lock();
            assert!(
                view.should_render_use_agent_footer(&model, ctx),
                "footer should render for viewer of shared ambient agent session with CLI agent",
            );
            // Contract change, `docs/DESIGN-PHOSPHOR-FORK.md` §8 step 3. This used to
            // assert that the footer's view id was the last rich content after the
            // active block -- i.e. that it had been injected *into the block list*
            // underneath the running command. That injection is the defect §8 exists to
            // remove: it occupies rows the pty was told it had and cannot see. The
            // footer is now rendered by the window footer bar, outside the block list,
            // so the assertion is inverted (nothing in the block list) and the identity
            // check moves to the bar's own per-frame predicate.
            assert!(
                model
                    .block_list()
                    .rich_content_row_range(view.use_agent_footer.id())
                    .is_none(),
                "the Use Agent footer must not be injected into the block list",
            );
            assert_eq!(
                view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                Some(view.use_agent_footer.id()),
                "the window footer bar should be rendering the Use Agent footer",
            );
        });
    })
}

/// Ported from the pinned oracle's `cli_agent_footer_does_not_render_for_warp_tui_session`.
/// The fork's variant is named `PhosphorTui`, not `WarpTui`; see #394. The footer
/// offering to hand a long-running command off to itself would be nonsensical when
/// the "long-running command" is this fork's own TUI.
#[test]
fn cli_agent_footer_does_not_render_for_phosphor_tui_session() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |view, ctx| {
            simulate_user_started_long_running_command(view);

            CLIAgentSessionsModel::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.set_session(
                    view.id(),
                    CLIAgentSession {
                        agent: CLIAgent::PhosphorTui,
                        status: CLIAgentSessionStatus::InProgress,
                        session_context: CLIAgentSessionContext::default(),
                        input_state: CLIAgentInputState::Closed,
                        listener: None,
                        plugin_version: None,
                        remote_host: None,
                        draft_text: None,
                        custom_command_prefix: None,
                        should_auto_toggle_input: false,
                        received_rich_notification: false,
                    },
                    ctx,
                );
            });

            view.refresh_use_agent_footer(ctx);

            let model = view.model.lock();
            assert!(!view.should_render_use_agent_footer(&model, ctx));
            let active_block_index = model.block_list().active_block_index();
            assert!(model
                .block_list()
                .last_non_hidden_rich_content_block_after_block(Some(active_block_index))
                .is_none());
            // Vacuous since §8 step 3; the bar's predicate is the live one.
            assert_eq!(
                view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                None,
            );
        });
    })
}

/// `docs/DESIGN-PHOSPHOR-FORK.md` §8 step 3: alt screen and blocklist mode now share one
/// predicate and one surface.
///
/// Before this change there were two. Blocklist mode got the toolbar from the block list,
/// where `maybe_show_use_agent_footer_in_blocklist` had inserted it below the running
/// command -- the rows-stealing defect §8 exists to remove -- and alt screen got it from a
/// column sibling added in `TerminalView::render` under
/// `model.is_alt_screen_active() && self.should_render_use_agent_footer(..)`, which is why
/// alt screen never had the bug. The column sibling is gone; both modes now come from
/// `use_agent_footer_view_id_for_window_footer_bar`.
#[test]
fn use_agent_footer_renders_from_the_window_footer_bar_in_alt_screen() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |view, ctx| {
            simulate_user_started_long_running_command(view);
            view.model.lock().set_altscreen_active();

            let view_id = view.id();
            CLIAgentSessionsModel::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.set_session(
                    view_id,
                    CLIAgentSession {
                        agent: CLIAgent::Claude,
                        status: CLIAgentSessionStatus::InProgress,
                        session_context: CLIAgentSessionContext::default(),
                        input_state: CLIAgentInputState::Closed,
                        listener: None,
                        plugin_version: None,
                        remote_host: None,
                        draft_text: None,
                        custom_command_prefix: None,
                        should_auto_toggle_input: false,
                        received_rich_notification: false,
                    },
                    ctx,
                );
            });

            view.refresh_use_agent_footer(ctx);

            let model = view.model.lock();
            assert!(model.is_alt_screen_active(), "precondition: alt screen");
            assert!(view.should_render_use_agent_footer(&model, ctx));
            assert!(
                model
                    .block_list()
                    .rich_content_row_range(view.use_agent_footer.id())
                    .is_none(),
                "alt screen never used the block list and still must not",
            );
            assert_eq!(
                view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                Some(view.use_agent_footer.id()),
                "the window footer bar renders the toolbar in alt screen too",
            );
        });
    })
}

/// The explicit hide survived the move off the block list.
///
/// Block-list membership was two things at once: the answer to
/// `should_render_use_agent_footer` at insertion time, *and* a piece of state that call
/// sites cleared deliberately (a spawned subagent, a completed block, tagging the agent
/// in). Only the first became a per-frame predicate; the second is
/// `use_agent_footer_suppressed`, and this pins it, because dropping it would have
/// silently widened when the toolbar shows.
///
/// Note what the middle assertion says: the predicate is still true. Suppression is not
/// the predicate, and it reserves nothing -- the bar is unconditional and fixed height
/// whether or not it has content (§8), so this flag can never affect the pty's rows.
#[test]
fn suppressing_the_use_agent_footer_empties_the_window_footer_bar() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);

        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |view, ctx| {
            simulate_user_started_long_running_command(view);
            transition_to_user_handoff_state(view, UserTakeOverReason::Manual, ctx);
            view.refresh_use_agent_footer(ctx);

            {
                let model = view.model.lock();
                assert_eq!(
                    view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                    Some(view.use_agent_footer.id()),
                );
            }

            view.suppress_use_agent_footer(ctx);
            {
                let model = view.model.lock();
                assert!(
                    view.should_render_use_agent_footer(&model, ctx),
                    "suppression is not the predicate -- the predicate is unchanged",
                );
                assert_eq!(
                    view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                    None,
                );
            }

            view.refresh_use_agent_footer(ctx);
            {
                let model = view.model.lock();
                assert_eq!(
                    view.use_agent_footer_view_id_for_window_footer_bar(&model, ctx),
                    Some(view.use_agent_footer.id()),
                );
            }
        });
    })
}
