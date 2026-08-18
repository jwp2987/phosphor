use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use instant::Instant;
use tempfile::TempDir;
use warp::appearance::Appearance;
use warp::settings::{
    AISettings, SettingsFileError, TuiStatuslineConfig, TuiStatuslineItem, TuiTheme,
    TuiThemeSettings, TuiZeroStateObject,
};
use warp::terminal::model::ansi::{Handler, InputBufferValue, Mode};
use warp::tui_export::{
    AIAgentAction, AIAgentActionId, AIAgentActionType, AIAgentExchangeId, AIAgentInput,
    AIAgentOutput, AIAgentOutputMessage, AIAgentOutputMessageType, AIAgentTodo, AIAgentTodoList,
    AIBlockModel, AIBlockOutputStatus, AIConversationAutoexecuteMode, AIConversationId,
    AIRequestType, AgentViewEntryOrigin, AgentViewState, AskUserQuestionItem,
    AskUserQuestionOption, AskUserQuestionType, BlockPadding, BlocklistAIHistoryEvent,
    BlocklistAIHistoryModel, ConversationStatus, Harness, InputType, LLMId, LLMPreferences,
    MessageId, OutputStatusUpdateCallback, PtyIntent, PtyIntentEvent, ServerOutputId, Session,
    Shared, SizeInfo, SizeUpdate, SlashCommandKind, TaskId, TuiMcpAction, TuiMcpServerId,
    TuiUpArrowHistoryItemKind, WarpConfig, WarpConfigUpdateEvent, export_conversation_markdown,
    queue_tui_permission_action, register_tui_session_view_test_singletons, slash_commands,
};
use warp_core::settings::Setting as _;
use warp_editor::model::CoreEditorModel;
use warpui::platform::WindowStyle;
use warpui::{
    AddWindowOptions, EntityIdMap, ModelHandle, ReadModel, SingletonEntity, UpdateModel, ViewHandle,
};
use warpui_core::elements::tui::{
    Color, TuiBuffer, TuiBufferExt, TuiConstrainedBox, TuiConstraint, TuiContainer, TuiElement,
    TuiEvent, TuiEventContext, TuiLayoutContext, TuiPaintContext, TuiPaintSurface, TuiPoint,
    TuiRect, TuiScene, TuiScreenPosition, TuiSize, TuiStyle, TuiText, TuiViewportPosition,
};
use warpui_core::r#async::Timer;
use warpui_core::event::ModifiersState;
use warpui_core::keymap::{Context, Keystroke, Trigger};
use warpui_core::presenter::tui::{TuiFrame, TuiPresenter};
use warpui_core::{
    App, AppContext, TuiView, TypedActionView as _, ViewContext, WindowInvalidation,
};

use super::statusline::{
    FooterSegment, FooterSegments, format_statusline_date, format_statusline_time_12_hour,
    format_statusline_time_24_hour, format_todo_progress, render_git_branch_status,
    render_status_footer_row, render_statusline_datetime, should_render_plain_git_branch,
};
use super::{
    ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME, AUTO_APPROVE_DISABLED_HINT,
    AUTO_APPROVE_ENABLED_HINT, AUTO_APPROVE_FEEDBACK_DURATION, AUTO_APPROVE_TOGGLE_BINDING_NAME,
    COST_CONVERSATION_IN_PROGRESS_HINT, COST_EMPTY_CONVERSATION_HINT,
    COST_NO_ACTIVE_CONVERSATION_HINT, CTRL_C_EXIT_HINT, CTRL_C_KILL_CHILD_HINT,
    ConversationRestoreState, DETACH_AGENT_FROM_RUNNING_COMMAND_BINDING_NAME,
    INLINE_MENU_TOP_PADDING_ROWS, LOADING_CONVERSATION_HINT, mcp_primary_action_hint,
    render_mcp_menu_footer,
    LOG_BUNDLE_FAILED_HINT, ORCHESTRATE_REQUIRES_CONVERSATION_HINT,
    ORCHESTRATE_REQUIRES_TASK_HINT, ORCHESTRATION_TAB_BAR_FOCUSED_FLAG,
    RUNNING_COMMAND_DETACH_HINT, SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG,
    SESSION_CAN_DETACH_AGENT_FROM_RUNNING_COMMAND_FLAG, SHELL_MODE_HINT,
    THEME_INVALID_ARGUMENT_HINT, TuiConversationRestoreOrigin, TuiQueuedFollowUp,
    TuiTerminalSessionAction, TuiTerminalSessionEvent, TuiTerminalSessionView,
    cost_command_unavailable_hint, export_file_success_message, log_bundle_success_message,
    raw_prompt_if_not_blank,
};
use crate::agent_block::{TuiAIBlock, TuiBlockingChild};
use crate::autoupdate::TuiAutoupdater;
use crate::inline_menu::MAX_INLINE_MENU_ROWS;
use crate::input_mode_policy::{AI_LOCKED_CONFIG, AI_UNLOCKED_CONFIG};
use crate::input_suggestions_mode::TuiInputSuggestionsMode;
use crate::keybindings::{
    CONTEXTUAL_PLAN_TOGGLE_BINDING_NAME, KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG,
    PLAN_TOGGLE_AVAILABLE_FLAG, PLAN_TOGGLE_BINDING_NAME, TUI_BINDING_GROUP,
};
use crate::orchestrated_agent_identity_styling::AgentIdentity;
use crate::orchestration_model::TuiOrchestrationModel;
use crate::orchestration_tab_bar::{
    orchestration_tab_icon, render_orchestration_child_selected_tab_footer,
    render_orchestration_tab_footer,
};
use crate::pane_group::TuiPaneGroup;
use crate::read_only_menu::TuiReadOnlyMenuKind;
use crate::root_view::RootTuiView;
use crate::session_registry::{TuiSessionId, TuiSessions};
use crate::statusline_config_view::TuiStatuslineConfigEvent;
use crate::terminal_block::{block_content_rows, should_render_terminal_block};
use crate::terminal_use::TuiInputTarget;
use crate::test_fixtures::{
    add_test_semantic_selection, add_test_terminal_session,
    add_test_terminal_session_with_settings_file_error,
};
use crate::transcript_view::TRANSCRIPT_BLOCK_SPACING;
use crate::tui_builder::TuiUiBuilder;
use crate::usage::render_context_usage_entry;
use crate::zero_state_animation::{
    ZeroStateAnimationConfig, ZeroStateAnimationConfigEvent, ZeroStateAnimationLoadFailure,
};

struct FocusTestFixture {
    window_id: warpui_core::WindowId,
    sessions: ModelHandle<TuiSessions>,
}

/// Ported from the pinned oracle (`02b53fcd8`). Guards
/// `super::attachment_focus_available`, the mutual-exclusion gate between the
/// `FocusAttachments` and `TriggerCompletions` Tab bindings: shell mode always
/// reserves Tab for completion, even while attachments would otherwise render.
#[test]
fn shell_mode_reserves_tab_even_when_attachments_render() {
    assert!(super::attachment_focus_available(false, true));
    assert!(!super::attachment_focus_available(true, true));
    assert!(!super::attachment_focus_available(false, false));
}

#[test]
fn shell_completion_source_warmup_loads_path_executables() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let session = Arc::new(Session::test());

        view.update(&mut app, |view, ctx| {
            view.warm_shell_completion_sources(session.clone(), ctx);
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while !session.has_loaded_external_commands() && Instant::now() < deadline {
            Timer::after(Duration::from_millis(10)).await;
        }

        assert!(session.has_attempted_to_load_external_commands());
        assert!(session.has_loaded_external_commands());
        assert!(session.executable_names().any(|command| command == "git"));
    });
}

#[test]
fn footer_supports_arbitrary_order_and_figma_group_dividers() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let row = render_status_footer_row(
                FooterSegments {
                    ordered: vec![
                        FooterSegment::ContextWindowUsage(render_context_usage_entry(0.426, ctx)),
                        FooterSegment::GitBranch("feature/statusline".to_owned()),
                        FooterSegment::ActiveIndicator("Auto-queue"),
                        FooterSegment::WorkingDirectory("/tmp/warp".to_owned()),
                        FooterSegment::DateTime(TuiText::new("July 20, 2026").finish()),
                    ],
                },
                &builder,
            )
            .finish();
            assert_eq!(
                render_element(row, ctx, 120).to_lines(),
                vec![
                    "43% context | feature/statusline | Auto-queue | /tmp/warp | July 20, 2026"
                        .to_owned()
                ],
            );

            let branch_only = render_status_footer_row(
                FooterSegments {
                    ordered: vec![FooterSegment::GitBranch("main".to_owned())],
                },
                &builder,
            )
            .finish();
            assert_eq!(
                render_element(branch_only, ctx, 80).to_lines(),
                vec!["main".to_owned()],
            );
        });
    });
}

/// Segments within the same Figma group (consecutive active indicators, a
/// working-directory/git-branch pair, consecutive date/time items) still use
/// " • " or their dedicated marker; segments from different, otherwise
/// unrelated groups use " | ".
#[test]
fn footer_uses_pipes_between_figma_groups_and_preserves_within_group_separators() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let row = render_status_footer_row(
                FooterSegments {
                    ordered: vec![
                        // The auto-approve entry is now the clickable `▶▶`
                        // control (`FooterSegment::AutoApproveIndicator`), not
                        // a plain label; it stays in the active-indicator
                        // group, so the " • " within-group join is unchanged.
                        FooterSegment::AutoApproveIndicator(TuiText::new("▶▶").finish()),
                        FooterSegment::ActiveIndicator("Auto-queue"),
                        FooterSegment::Model(TuiText::new("model").finish()),
                        FooterSegment::WorkingDirectory("/tmp/warp".to_owned()),
                        FooterSegment::GitBranch("main".to_owned()),
                        FooterSegment::GitDiff {
                            files_changed: 6,
                            additions: 31,
                            deletions: 12,
                        },
                        FooterSegment::GitHubPullRequest(TuiText::new("PR #123").finish()),
                        FooterSegment::ContextWindowUsage(render_context_usage_entry(0.426, ctx)),
                        FooterSegment::DateTime(TuiText::new("July 20, 2026").finish()),
                        FooterSegment::DateTime(TuiText::new("1:08pm").finish()),
                        FooterSegment::AgentTodoList(TuiText::new("❒ 1/10").finish()),
                    ],
                },
                &builder,
            )
            .finish();
            assert_eq!(
                render_element(row, ctx, 160).to_lines(),
                vec![
                    "▶▶ • Auto-queue | model | /tmp/warp ↬ main | ☰ 6 • +31 -12 | PR #123 | 43% context | July 20, 2026 • 1:08pm | ❒ 1/10"
                        .to_owned()
                ],
            );
        });
    });
}

#[test]
fn empty_configurable_footer_has_zero_height() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let row = render_status_footer_row(
                FooterSegments {
                    ordered: Vec::new(),
                },
                &builder,
            )
            .finish();
            assert!(render_element(row, ctx, 80).to_lines().is_empty());
        });
    });
}

/// Adapted from warp/master: upstream's `AutoQueue` item reflects a
/// persistent "auto-queue next prompt" *mode* (`QueuedQueryModel`), a
/// feature Zap has not ported. Zap's `/queue` instead holds one specific
/// prompt (`TuiTerminalSessionView::queued_follow_up`), so this test drives
/// that field directly instead of `QueuedQueryModel`.
///
/// The two enabled items now behave differently, which is what this asserts:
/// `Queued` is a presence-only indicator and disappears when no follow-up is
/// held, while `AutoApprove` is the pin's clickable `▶▶` control and stays on
/// the row in both states so it can be clicked to turn auto-approve on. Its
/// state is carried by colour, asserted in
/// `enabled_auto_approve_indicator_is_always_visible_with_state_aware_color`.
#[test]
fn enabled_auto_indicators_reflect_their_effective_states() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            // A fresh session's input defaults to `InputType::Shell` (see
            // `BlocklistAIInputModel::new`) until the user types or NLD
            // classifies otherwise. `render_footer` correctly hides the
            // agent-only Auto-approve/Auto-queue indicators in shell mode
            // (`FooterSegment::ActiveIndicator` is only pushed when
            // `!shell_mode`), so this test — which is about those indicators
            // specifically — must switch to AI input mode first, the same
            // way a real conversation entry would.
            view.ai_input_model.update(ctx, |input_model, ctx| {
                input_model.set_input_type(InputType::AI, ctx);
            });
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection
                    .try_start_new_conversation(AgentViewEntryOrigin::Tui, ctx)
                    .expect("test conversation should start")
            });
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.toggle_pending_query_autoexecute(ctx);
            });
            view.queued_follow_up = Some(TuiQueuedFollowUp {
                conversation_id: view
                    .conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx)
                    .expect("test conversation should be selected"),
                prompt: "later".to_owned(),
                seen_in_progress: false,
            });
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .tui_statusline
                    .set_value(
                        TuiStatuslineConfig {
                            order: vec![
                                TuiStatuslineItem::AutoApprove,
                                TuiStatuslineItem::AutoQueue,
                            ],
                            enabled: vec![
                                TuiStatuslineItem::AutoApprove,
                                TuiStatuslineItem::AutoQueue,
                            ],
                        }
                        .normalized(),
                        ctx,
                    )
                    .expect("statusline setting should persist");
            });
        });

        // "Queued" (not upstream's "Auto-queue") matches the re-backed
        // semantics documented on `TuiStatuslineItem`/above: this reflects a
        // one-shot queued follow-up prompt, not a persistent auto-queue mode.
        assert_eq!(
            render_footer_lines(&mut app, &view, 80),
            vec!["▶▶ • Queued".to_owned()],
        );

        view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.toggle_pending_query_autoexecute(ctx);
            });
            view.queued_follow_up = None;
        });
        // The queued follow-up indicator is gone, but the auto-approve control
        // remains: it is a toggle, not a status label, so switching
        // auto-approve off must not remove its own click target.
        assert_eq!(
            render_footer_lines(&mut app, &view, 80),
            vec!["▶▶".to_owned()],
        );

        // See the comment in `saving_statusline_configuration_persists_and_restores_input_focus`:
        // `tui_statusline` persists across tests in this process, so restore
        // the default before this test ends.
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings
                    .tui_statusline
                    .set_value(TuiStatuslineConfig::default(), ctx);
            });
        });
    });
}

#[test]
fn shortcuts_surface_renders_above_the_input() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.suggestions_mode.update(ctx, |mode, ctx| {
                mode.set_mode(
                    TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Shortcuts),
                    ctx,
                );
            });
        });

        let rendered = render_session(&mut app, &view, 80, 24).join("\n");
        assert!(rendered.contains("Shortcuts"), "{rendered}");
        assert!(rendered.contains("? shortcuts"), "{rendered}");
        assert!(rendered.contains("/ commands"), "{rendered}");
        assert!(rendered.contains("! shell mode"), "{rendered}");
        assert!(rendered.contains("← conversations"), "{rendered}");
        assert!(rendered.contains("↑ input history"), "{rendered}");
        // The shortcuts panel must NOT include the status section (that
        // lives in the dedicated status menu opened by /status).
        assert!(
            !rendered.contains("Version"),
            "Shortcuts panel must not show Version:\n{rendered}"
        );
        assert!(
            !rendered.contains("Working directory"),
            "Shortcuts panel must not show Working directory:\n{rendered}"
        );
    });
}

/// Regression coverage for a real divergence found by the #2 sweep, not a straight
/// port. The pin's `handle_interrupt` closes any open read-only sheet before it does
/// anything else; this fork's did not, so ctrl-c left the `?` shortcuts sheet (and the
/// `/status` menu) painted over the session while the interrupt did its work
/// underneath, and only a second, unrelated keystroke cleared it. See the comment on
/// the `suggestions_mode` block in `handle_interrupt`.
///
/// The pin asserts this through `terminal_use_interrupt_closes_shortcuts_before_taking_control`,
/// which additionally sets up an agent-monitored long-running command and checks that
/// control transferred to the user. That half is unaffected by the fix and already has
/// its own coverage (`terminal_use_interrupt_follows_takeover_then_process_interrupt_policy`
/// in `terminal_use_tests.rs`), so this pins the part that was broken, on the path any
/// ctrl-c takes.
#[test]
fn interrupt_closes_an_open_read_only_menu() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.suggestions_mode.update(ctx, |mode, ctx| {
                mode.set_mode(
                    TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Shortcuts),
                    ctx,
                );
            });
        });
        view.read(&app, |view, ctx| {
            assert_eq!(
                view.suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Shortcuts)
            );
        });

        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        view.read(&app, |view, ctx| {
            assert_eq!(
                view.suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::Closed,
                "ctrl-c should dismiss the shortcuts sheet"
            );
        });
    });
}

/// The same, for the `/status` sheet -- both are `ReadOnlyMenu` kinds and the fix is
/// kind-agnostic, so pin both rather than leaving the other kind to drift.
#[test]
fn interrupt_closes_an_open_status_menu() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.suggestions_mode.update(ctx, |mode, ctx| {
                mode.set_mode(
                    TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Status),
                    ctx,
                );
            });
        });

        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        view.read(&app, |view, ctx| {
            assert_eq!(
                view.suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::Closed,
                "ctrl-c should dismiss the status menu"
            );
        });
    });
}

#[test]
fn status_slash_command_opens_the_status_menu() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::STATUS, None, ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                view.as_ref(ctx).suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Status)
            );
        });

        let rendered = render_session(&mut app, &view, 80, 24).join("\n");
        assert!(rendered.contains("Status"), "{rendered}");
        assert!(rendered.contains("Version"), "{rendered}");
        assert!(rendered.contains("Session"), "{rendered}");
        assert!(rendered.contains("Conversation ID"), "{rendered}");
        assert!(rendered.contains("Working directory"), "{rendered}");
        // Cloud account fields dropped -- BYOP has no org or sign-in email.
        assert!(!rendered.contains("Org"), "{rendered}");
        assert!(!rendered.contains("Email"), "{rendered}");
    });
}

/// `/api-keys` opens the inline key manager in place, flips the shared suggestions mode over to
/// it, and consumes the typed command so the composer is left empty (and the command text is no
/// longer painted anywhere on screen).
///
/// The pin additionally asserts on its fixed cloud provider list ("Anthropic API key", "enter to
/// set api key", no "ctrl + x"). This fork's menu is BYOP: it lists whatever agent providers the
/// user has configured, with no built-in provider entries, so those strings have no counterpart
/// here and the header assertion stands in their place.
#[test]
fn api_keys_slash_command_opens_inline_and_clears_the_input() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.input_view
                .update(ctx, |input, ctx| input.set_text("/api-keys", ctx));
            view.execute_tui_slash_command(&slash_commands::API_KEYS, None, ctx);
        });

        view.read(&app, |view, ctx| {
            assert!(view.api_keys_menu.as_ref(ctx).is_open(ctx));
            assert_eq!(
                view.suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::ApiKeys
            );
            assert!(view.input_view.as_ref(ctx).is_empty(ctx));
        });
        let rendered = render_session(&mut app, &view, 100, 40).join("\n");
        assert!(rendered.contains("API keys"), "{rendered}");
        assert!(!rendered.contains("/api-keys"), "{rendered}");
    });
}

/// `/fork` is one of the commands both front-ends implement, so neither surface filter may drop
/// it. This fork derives surfaces from the command name rather than carrying the pin's
/// `supported_surfaces` field, so the same claim is made through `supports_gui`/`supports_tui`.
#[test]
fn fork_slash_command_is_available_on_both_surfaces() {
    assert_eq!(slash_commands::FORK.name, "/fork");
    assert!(slash_commands::FORK.supports_gui());
    assert!(slash_commands::FORK.supports_tui());
}

/// `/fork` needs something to branch from, so it must refuse — with a hint, not a silent
/// no-op — when the session has nothing selected, and must still consume the typed command.
///
/// The pin calls the refusal `FORK_EMPTY_CONVERSATION_HINT`; this fork's single guard is
/// `FORK_REQUIRES_CONVERSATION_HINT` (`fork_current_conversation`'s `let Some(source) = …
/// else`). The user-visible situation is the same one: a TUI session that has never submitted
/// a prompt has no selected conversation at all, which is what "empty conversation" means
/// here.
#[test]
fn fork_slash_command_rejects_an_empty_conversation() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.input_view
                .update(ctx, |input, ctx| input.set_text("/fork", ctx));
            view.execute_tui_slash_command(&slash_commands::FORK, None, ctx);
        });

        view.read(&app, |view, ctx| {
            assert_eq!(
                view.transient_hint.current().map(|(text, _)| text),
                Some(super::FORK_REQUIRES_CONVERSATION_HINT),
            );
            assert!(view.input_view.as_ref(ctx).is_empty(ctx));
        });
    });
}

#[test]
fn statusline_slash_command_clears_input_focuses_one_picker_and_cancels_cleanly() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("/statusline", ctx);
            });
            view.execute_tui_slash_command(&slash_commands::STATUSLINE, None, ctx);
        });

        let (picker_id, picker_focus_id) = view.read(&app, |view, ctx| {
            let picker = view
                .statusline_config_view
                .as_ref()
                .expect("statusline picker should be open");
            assert!(view.input_view.as_ref(ctx).is_empty(ctx));
            assert!(picker.as_ref(ctx).is_focused(ctx));
            (
                picker.id(),
                ctx.focused_view_id(fixture.window_id)
                    .expect("the statusline picker owns focus while it is open"),
            )
        });

        // A terminal redraw must leave the open interaction surface's focus
        // alone; before this, the wakeup re-ran the full focus reconciliation
        // and re-focused the picker itself, dropping any focus it had
        // delegated to a child row.
        assert!(view.update(&mut app, |view, ctx| { view.handle_terminal_wakeup(ctx) }));
        app.read(|ctx| {
            assert_eq!(
                ctx.focused_view_id(fixture.window_id),
                Some(picker_focus_id),
                "a redraw must preserve the interaction surface's focus"
            );
        });

        // A second `/statusline` while one is already open does not mount a
        // second picker.
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::STATUSLINE, None, ctx);
        });
        assert_eq!(
            view.read(&app, |view, _| {
                view.statusline_config_view.as_ref().map(ViewHandle::id)
            }),
            Some(picker_id),
        );

        view.update(&mut app, |view, ctx| {
            view.handle_statusline_config_event(&TuiStatuslineConfigEvent::Cancelled, ctx);
        });
        view.read(&app, |view, ctx| {
            assert!(view.statusline_config_view.is_none());
            assert!(view.input_view.is_focused(ctx));
        });
    });
}

/// The stricter sibling of the test above: focus is asserted through
/// `check_view_or_child_focused`, so the picker is allowed to delegate focus down to a child row
/// and a redraw still has to leave that *nested* focus alone. Dismissing the surface must then
/// hand focus back to the input and — the part rendering-only assertions miss — bring the input's
/// terminal cursor back with it.
#[test]
fn statusline_slash_command_preserves_nested_focus_and_restores_the_input_cursor() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("/statusline", ctx);
            });
            view.execute_tui_slash_command(&slash_commands::STATUSLINE, None, ctx);
        });

        let (picker_id, picker_focus_id) = view.read(&app, |view, ctx| {
            let picker = view
                .statusline_config_view
                .as_ref()
                .expect("statusline picker should be open");
            assert!(view.input_view.as_ref(ctx).is_empty(ctx));
            assert!(ctx.check_view_or_child_focused(fixture.window_id, &picker.id()));
            (
                picker.id(),
                ctx.focused_view_id(fixture.window_id)
                    .expect("the statusline picker should own or delegate focus"),
            )
        });

        assert!(view.update(&mut app, |view, ctx| { view.handle_terminal_wakeup(ctx) }));
        app.read(|ctx| {
            assert_eq!(
                ctx.focused_view_id(fixture.window_id),
                Some(picker_focus_id),
                "a redraw must preserve the interaction surface's delegated child focus"
            );
        });

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::STATUSLINE, None, ctx);
        });
        assert_eq!(
            view.read(&app, |view, _| {
                view.statusline_config_view.as_ref().map(ViewHandle::id)
            }),
            Some(picker_id),
        );

        view.update(&mut app, |view, ctx| {
            view.handle_statusline_config_event(&TuiStatuslineConfigEvent::Cancelled, ctx);
        });
        view.read(&app, |view, ctx| {
            assert!(view.statusline_config_view.is_none());
            assert!(ctx.check_view_or_child_focused(fixture.window_id, &view.input_view.id()));
        });
        assert!(
            render_session_frame(&mut app, &view, 80, 24)
                .cursor
                .is_some(),
            "dismissing the interaction surface should restore the input cursor"
        );
    });
}

#[test]
fn saving_statusline_configuration_persists_and_restores_input_focus() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let config = TuiStatuslineConfig {
            order: vec![
                TuiStatuslineItem::ContextWindowUsage,
                TuiStatuslineItem::GitBranch,
            ],
            enabled: Vec::new(),
        }
        .normalized();

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::STATUSLINE, None, ctx);
            view.handle_statusline_config_event(
                &TuiStatuslineConfigEvent::Saved(config.clone()),
                ctx,
            );
        });

        assert_eq!(
            app.read(|ctx| AISettings::as_ref(ctx).tui_statusline.normalized()),
            config,
        );
        view.read(&app, |view, ctx| {
            assert!(view.statusline_config_view.is_none());
            assert!(view.input_view.is_focused(ctx));
            assert_eq!(
                view.transient_hint.current().map(|(text, _)| text),
                Some(super::STATUSLINE_SAVED_HINT),
            );
        });

        // `tui_statusline` persists through the same on-disk settings path
        // `AISettings` uses outside tests, so leaving a non-default value set
        // here would leak into whichever test runs next in this process.
        // Restore the default so other footer tests keep seeing it.
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings
                    .tui_statusline
                    .set_value(TuiStatuslineConfig::default(), ctx);
            });
        });
    });
}

/// `/reset-statusline` is the escape hatch from a hand-edited configuration: it must put
/// *both* halves — which items are enabled and the order they sit in — back to the shipped
/// default, without opening the picker, and confirm through the same hint slot the save path
/// uses. It also consumes the typed command like every other executed slash command.
///
/// The pin's custom fixture is built from its cloud `CreditUsage` item; this fork has no such
/// statusline item (BYOP has no credits), so the same "not the default, in a different order"
/// shape is built from `ContextWindowUsage` — the entry that replaced it.
#[test]
fn reset_statusline_command_restores_default_items_and_ordering() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let custom = TuiStatuslineConfig {
            order: vec![
                TuiStatuslineItem::ContextWindowUsage,
                TuiStatuslineItem::Model,
            ],
            enabled: vec![TuiStatuslineItem::ContextWindowUsage],
        }
        .normalized();
        assert_ne!(custom, TuiStatuslineConfig::default());

        view.update(&mut app, |view, ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .tui_statusline
                    .set_value(custom, ctx)
                    .expect("custom statusline should persist");
            });
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("/reset-statusline", ctx);
            });
            view.execute_tui_slash_command(&slash_commands::RESET_STATUSLINE, None, ctx);
        });

        assert_eq!(
            app.read(|ctx| AISettings::as_ref(ctx).tui_statusline.normalized()),
            TuiStatuslineConfig::default(),
        );
        view.read(&app, |view, ctx| {
            assert!(view.statusline_config_view.is_none());
            assert!(view.input_view.as_ref(ctx).is_empty(ctx));
            assert_eq!(
                view.transient_hint.current().map(|(text, _)| text),
                Some(super::STATUSLINE_RESET_HINT),
            );
        });
    });
}

#[test]
fn log_bundle_success_message_includes_the_absolute_path() {
    let path = std::path::Path::new("/tmp/warp-20260718-132640.zip");
    assert_eq!(
        log_bundle_success_message(path),
        "Log bundle saved to /tmp/warp-20260718-132640.zip"
    );
}

#[test]
fn log_bundle_failure_hint_does_not_hardcode_a_frontend_path() {
    assert!(!LOG_BUNDLE_FAILED_HINT.contains("warp.log"));
    assert!(!LOG_BUNDLE_FAILED_HINT.contains("/oz/"));
    assert!(!LOG_BUNDLE_FAILED_HINT.contains("/tui/"));
    assert!(!LOG_BUNDLE_FAILED_HINT.contains("/warp-cli/"));
}
#[test]
fn inline_menu_padding_preserves_result_capacity() {
    App::test((), |app| async move {
        app.read(|ctx| {
            let menu_rows = (0..MAX_INLINE_MENU_ROWS)
                .map(|row| format!("menu {row}"))
                .collect::<Vec<_>>();
            let menu = TuiConstrainedBox::new(
                TuiContainer::new(TuiText::new(menu_rows.join("\n")).finish())
                    .with_padding_top(INLINE_MENU_TOP_PADDING_ROWS)
                    .finish(),
            )
            .with_max_rows(MAX_INLINE_MENU_ROWS + INLINE_MENU_TOP_PADDING_ROWS)
            .finish();
            let lines = render_element_with_size(
                menu,
                ctx,
                20,
                MAX_INLINE_MENU_ROWS + INLINE_MENU_TOP_PADDING_ROWS,
            )
            .to_lines();

            assert_eq!(lines.len(), usize::from(MAX_INLINE_MENU_ROWS + 1));
            assert!(lines[0].trim().is_empty());
            assert_eq!(&lines[1..], menu_rows);
        });
    });
}

fn mouse_moved(x: u16, y: u16) -> TuiEvent {
    TuiEvent::MouseMoved {
        position: TuiPoint::new(x, y),
        modifiers: ModifiersState::default(),
        is_synthetic: false,
    }
}

fn left_mouse_down(x: u16, y: u16) -> TuiEvent {
    TuiEvent::LeftMouseDown {
        position: TuiPoint::new(x, y),
        modifiers: ModifiersState::default(),
        click_count: 1,
        is_first_mouse: false,
    }
}

fn left_mouse_up(x: u16, y: u16) -> TuiEvent {
    TuiEvent::LeftMouseUp {
        position: TuiPoint::new(x, y),
        modifiers: ModifiersState::default(),
    }
}

/// Lays out, paints and retains an already-built element so a test can
/// dispatch mouse events against the same element + scene the paint produced.
/// Ported from the pin's helper of the same name; `render_retained_session`
/// below is the whole-session-view caller it was extracted from.
fn render_retained_element(
    mut element: Box<dyn TuiElement>,
    ctx: &AppContext,
    width: u16,
    height: u16,
) -> (Box<dyn TuiElement>, Rc<TuiScene>, TuiBuffer) {
    let mut rendered_views = EntityIdMap::default();
    let mut layout_ctx = TuiLayoutContext {
        rendered_views: &mut rendered_views,
    };
    let size = element.layout(
        TuiConstraint::loose(TuiSize::new(width, height)),
        &mut layout_ctx,
        ctx,
    );
    element.after_layout(&mut layout_ctx, ctx);
    let area = TuiRect::new(0, 0, size.width.min(width), size.height.min(height));
    let mut buffer = TuiBuffer::empty(area);
    let mut paint_ctx = TuiPaintContext::new(&mut rendered_views);
    {
        let mut surface = TuiPaintSurface::new(&mut buffer);
        element.render(TuiScreenPosition::new(0, 0), &mut surface, &mut paint_ctx);
    }
    let scene = Rc::new(paint_ctx.scene.clone());
    (element, scene, buffer)
}

/// Renders the session view's element tree outside the presenter so the test
/// can dispatch mouse events against the retained element + scene. Child views
/// (transcript/input/attachment bar) are absent from `rendered_views`, so they
/// lay out zero-size; the footer — part of the session view's own tree —
/// renders with the clickable model label.
fn render_retained_session(
    app: &App,
    view: &ViewHandle<super::TuiTerminalSessionView>,
    width: u16,
    height: u16,
) -> (Box<dyn TuiElement>, Rc<TuiScene>, TuiBuffer) {
    app.read(|ctx| {
        let element = ctx
            .render_tui_view(view.window_id(ctx), view.id())
            .expect("session view should render");
        render_retained_element(element, ctx, width, height)
    })
}

/// Dispatches `event` into the retained session element tree with the session
/// view as the action origin, returning whether the tree handled it.
fn dispatch_session_event(
    app: &App,
    view: &ViewHandle<super::TuiTerminalSessionView>,
    element: &mut Box<dyn TuiElement>,
    scene: Rc<TuiScene>,
    event: &TuiEvent,
) -> bool {
    app.read(|ctx| {
        let mut rendered_views = EntityIdMap::default();
        let mut event_ctx = TuiEventContext::new(scene, &mut rendered_views);
        event_ctx.set_origin_view(Some(view.id()));
        element.dispatch_event(event, &mut event_ctx, ctx)
    })
}

/// Locates a footer label in the rendered buffer, returning the (column, row)
/// of its first cell. Counts chars (not bytes) so multi-byte glyphs earlier in
/// the footer row don't shift the column.
fn footer_label_position(buffer: &TuiBuffer, label: &str) -> (u16, u16) {
    let lines = buffer.to_lines();
    for (row, line) in lines.iter().enumerate() {
        if let Some(byte_offset) = line.find(label) {
            let col = line[..byte_offset].chars().count() as u16;
            return (col, row as u16);
        }
    }
    panic!(
        "label {:?} not found in rendered footer:\n{}",
        label,
        lines.join("\n")
    );
}

#[test]
fn toggle_model_menu_action_opens_and_closes_the_inline_model_menu() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.read(&app, |view, ctx| {
            assert!(
                !view.model_menu.as_ref(ctx).is_open(ctx),
                "model menu should start closed"
            );
        });
        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::ToggleModelMenu, ctx);
        });
        view.read(&app, |view, ctx| {
            assert!(
                view.model_menu.as_ref(ctx).is_open(ctx),
                "ToggleModelMenu action should open a closed inline model menu"
            );
        });
        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::ToggleModelMenu, ctx);
        });
        view.read(&app, |view, ctx| {
            assert!(
                !view.model_menu.as_ref(ctx).is_open(ctx),
                "ToggleModelMenu action should close an open inline model menu"
            );
        });
    });
}

fn todo(id: &str, title: &str) -> AIAgentTodo {
    AIAgentTodo::new(id.to_owned().into(), title.to_owned(), String::new())
}

fn set_selected_todo_list(
    app: &mut App,
    view: &ViewHandle<super::TuiTerminalSessionView>,
    completed: Vec<AIAgentTodo>,
    pending: Vec<AIAgentTodo>,
    status: ConversationStatus,
) -> AIConversationId {
    view.update(app, |view, ctx| {
        let conversation_id = view.conversation_selection.update(ctx, |selection, ctx| {
            selection
                .try_start_new_conversation(AgentViewEntryOrigin::Tui, ctx)
                .expect("test conversation should start")
        });
        let terminal_surface_id = view.terminal_surface_id;
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            let conversation = history
                .conversation_mut(&conversation_id)
                .expect("selected conversation should exist");
            conversation.set_todo_lists_for_test(vec![
                AIAgentTodoList::default()
                    .with_completed_items(completed)
                    .with_pending_items(pending),
            ]);
            conversation.update_status(status, terminal_surface_id, ctx);
        });
        conversation_id
    })
}

fn set_enabled_statusline_items(app: &mut App, items: Vec<TuiStatuslineItem>) {
    app.update(|ctx| {
        AISettings::handle(ctx).update(ctx, |settings, ctx| {
            settings
                .tui_statusline
                .set_value(
                    TuiStatuslineConfig {
                        order: items.clone(),
                        enabled: items,
                    }
                    .normalized(),
                    ctx,
                )
                .expect("statusline setting should persist");
        });
    });
}

/// `tui_statusline` persists across tests in this process (see the comment in
/// `saving_statusline_configuration_persists_and_restores_input_focus`), so
/// every test that calls `set_enabled_statusline_items` must restore the
/// default before it ends.
fn restore_default_statusline(app: &mut App) {
    app.update(|ctx| {
        AISettings::handle(ctx).update(ctx, |settings, ctx| {
            let _ = settings
                .tui_statusline
                .set_value(TuiStatuslineConfig::default(), ctx);
        });
    });
}

#[test]
fn todo_menu_renders_active_list_and_toggles_through_shared_suggestions_mode() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        set_enabled_statusline_items(&mut app, vec![TuiStatuslineItem::AgentTodoList]);
        set_selected_todo_list(
            &mut app,
            &view,
            vec![todo("done", "Completed task")],
            vec![todo("current", "Current task"), todo("later", "Later task")],
            ConversationStatus::InProgress,
        );

        assert_eq!(render_footer_lines(&mut app, &view, 80), vec!["❒ 1/3"]);
        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::ToggleTodoMenu, ctx);
        });
        view.read(&app, |view, ctx| {
            assert_eq!(
                view.suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Todos)
            );
            assert_eq!(
                view.read_only_menu_viewport.position(),
                TuiViewportPosition::RowsFromTop(2),
                "the title and completed row precede the current task"
            );
        });

        let rendered = render_session(&mut app, &view, 80, 24).join("\n");
        let completed = rendered.find("✓ Completed task").unwrap();
        // `●` (U+25CF), matching both pins. The fork had diverged to the
        // narrower `•` (U+2022), recorded nowhere (#584), while its sibling
        // arms (`✓`, `■`) matched the pin exactly. The maintainer adopted the
        // pin's glyph, so this assertion moved with it.
        let current = rendered.find("● Current task").unwrap();
        let later = rendered.find("◌ Later task").unwrap();
        assert!(rendered.contains("Tasks 1/3"));
        assert!(completed < current && current < later);

        view.update(&mut app, |view, ctx| {
            view.suggestions_mode.update(ctx, |mode, ctx| {
                mode.set_mode(
                    TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Status),
                    ctx,
                );
            });
            view.handle_action(&TuiTerminalSessionAction::ToggleTodoMenu, ctx);
            assert_eq!(
                view.suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Todos)
            );
            view.handle_action(&TuiTerminalSessionAction::ToggleTodoMenu, ctx);
            assert_eq!(
                view.suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::Closed
            );
        });
        restore_default_statusline(&mut app);
    });
}

#[test]
fn finished_todo_list_remains_visible_and_openable() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        set_enabled_statusline_items(&mut app, vec![TuiStatuslineItem::AgentTodoList]);
        set_selected_todo_list(
            &mut app,
            &view,
            vec![todo("done", "Completed task")],
            Vec::new(),
            ConversationStatus::Success,
        );

        assert_eq!(render_footer_lines(&mut app, &view, 80), vec!["✓ 1/1"]);
        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::ToggleTodoMenu, ctx);
        });
        let rendered = render_session(&mut app, &view, 80, 24).join("\n");
        assert!(rendered.contains("Tasks 1/1"));
        assert!(rendered.contains("✓ Completed task"));
        restore_default_statusline(&mut app);
    });
}

#[test]
fn todo_updates_preserve_scroll_and_close_the_menu_when_the_list_disappears() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let conversation_id = set_selected_todo_list(
            &mut app,
            &view,
            Vec::new(),
            vec![todo("current", "Current task")],
            ConversationStatus::InProgress,
        );
        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::ToggleTodoMenu, ctx);
            view.read_only_menu_viewport.scroll_to_rows_from_top(4);
            view.handle_history_event(
                &BlocklistAIHistoryEvent::UpdatedTodoList {
                    terminal_view_id: view.terminal_surface_id,
                },
                ctx,
            );
            assert_eq!(
                view.read_only_menu_viewport.position(),
                TuiViewportPosition::RowsFromTop(4)
            );
            view.handle_action(
                &TuiTerminalSessionAction::ToggleAutoApprove {
                    show_feedback: false,
                },
                ctx,
            );
            assert_eq!(
                view.read_only_menu_viewport.position(),
                TuiViewportPosition::RowsFromTop(4)
            );

            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, _| {
                history
                    .conversation_mut(&conversation_id)
                    .unwrap()
                    .set_todo_lists_for_test(vec![
                        AIAgentTodoList::default()
                            .with_pending_items(vec![todo("old", "Old task")]),
                        AIAgentTodoList::default()
                            .with_completed_items(vec![todo("done", "Completed task")])
                            .with_pending_items(vec![todo("new", "New current task")]),
                    ]);
            });
            view.handle_history_event(
                &BlocklistAIHistoryEvent::UpdatedTodoList {
                    terminal_view_id: view.terminal_surface_id,
                },
                ctx,
            );
            assert_eq!(
                view.read_only_menu_viewport.position(),
                TuiViewportPosition::RowsFromTop(2)
            );

            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, _| {
                history
                    .conversation_mut(&conversation_id)
                    .unwrap()
                    .set_todo_lists_for_test(Vec::new());
            });
            view.handle_history_event(
                &BlocklistAIHistoryEvent::UpdatedTodoList {
                    terminal_view_id: view.terminal_surface_id,
                },
                ctx,
            );
            assert_eq!(
                view.suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::Closed
            );
        });
    });
}

#[test]
fn footer_todo_item_is_a_bounded_click_target() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        set_enabled_statusline_items(&mut app, vec![TuiStatuslineItem::AgentTodoList]);
        set_selected_todo_list(
            &mut app,
            &view,
            Vec::new(),
            vec![todo("current", "Current task")],
            ConversationStatus::InProgress,
        );
        let (mut element, scene, buffer) = render_retained_session(&app, &view, 40, 20);
        let (todo_col, todo_row) = footer_label_position(&buffer, "❒ 0/1");
        let inside = (todo_col + 1, todo_row);
        let outside = (todo_col + 6, todo_row);

        dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &mouse_moved(inside.0, inside.1),
        );
        assert!(view.read(&app, |view, _| {
            view.todo_list_mouse.lock().unwrap().is_hovered()
        }));
        assert!(dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &left_mouse_down(inside.0, inside.1),
        ));
        assert!(dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &left_mouse_up(inside.0, inside.1),
        ));
        assert!(!view.read(&app, |view, _| {
            view.todo_list_mouse.lock().unwrap().is_clicked()
        }));

        dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &mouse_moved(outside.0, outside.1),
        );
        assert!(!view.read(&app, |view, _| {
            view.todo_list_mouse.lock().unwrap().is_hovered()
        }));
        assert!(!dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene,
            &left_mouse_down(outside.0, outside.1),
        ));
        restore_default_statusline(&mut app);
    });
}

#[test]
fn cost_command_uses_the_gui_eligibility_rules() {
    assert_eq!(
        cost_command_unavailable_hint(None),
        Some(COST_NO_ACTIVE_CONVERSATION_HINT),
    );
    assert_eq!(
        cost_command_unavailable_hint(Some((true, false))),
        Some(COST_EMPTY_CONVERSATION_HINT),
    );
    assert_eq!(
        cost_command_unavailable_hint(Some((false, false))),
        Some(COST_CONVERSATION_IN_PROGRESS_HINT),
    );
    assert_eq!(cost_command_unavailable_hint(Some((false, true))), None);
}

/// Renders the agent-mode footer row (`render_status_footer_row` + the real
/// `render_context_usage_entry`) to text lines at a fixed context fraction.
fn render_usage_footer_row(app: &mut App, context_fraction: f32) -> Vec<String> {
    app.update(|ctx| {
        let builder = TuiUiBuilder::from_app(ctx);
        let usage = render_context_usage_entry(context_fraction, ctx);
        let row = render_status_footer_row(
            FooterSegments {
                ordered: vec![
                    FooterSegment::Model(
                        TuiText::new("TestModel")
                            .with_style(builder.primary_text_style())
                            .truncate()
                            .finish(),
                    ),
                    FooterSegment::ContextWindowUsage(usage),
                ],
            },
            &builder,
        )
        .finish();
        render_element(row, ctx, 60).to_lines()
    })
}

#[test]
fn response_summary_visibility_is_independent_from_the_footer_usage_entry() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let exchange_id = AIAgentExchangeId::new();

        let context_fraction = 0.42_f32;

        let footer_before = render_usage_footer_row(&mut app, context_fraction);
        let summary_before = view.read(&app, |view, ctx| {
            view.render_response_summary_for_exchange(
                exchange_id,
                Duration::from_secs(2),
                Some(3.0),
                ctx,
            )
            .map(|summary| render_element(summary, ctx, 60).to_lines())
        });
        assert_eq!(summary_before, Some(vec!["∷ 2s • 3 credits".to_owned()]),);

        view.update(&mut app, |view, _| {
            view.toggle_response_summary_visibility_for_exchange(exchange_id);
        });
        let summary_hidden = view.read(&app, |view, ctx| {
            view.render_response_summary_for_exchange(
                exchange_id,
                Duration::from_secs(2),
                Some(3.0),
                ctx,
            )
        });
        assert!(summary_hidden.is_none());
        assert_eq!(
            render_usage_footer_row(&mut app, context_fraction),
            footer_before,
            "hiding the response summary must not change the footer usage entry",
        );

        view.update(&mut app, |view, _| {
            view.toggle_response_summary_visibility_for_exchange(exchange_id);
        });
        let summary_again = view.read(&app, |view, ctx| {
            view.render_response_summary_for_exchange(
                exchange_id,
                Duration::from_secs(2),
                Some(3.0),
                ctx,
            )
            .map(|summary| render_element(summary, ctx, 60).to_lines())
        });
        assert_eq!(summary_again, Some(vec!["∷ 2s • 3 credits".to_owned()]),);
    });
}

#[test]
fn auto_approve_slash_command_toggles_selected_conversation_off_on_off() {
    App::test((), |mut app| async move {
        assert_eq!(
            AUTO_APPROVE_FEEDBACK_DURATION,
            std::time::Duration::from_secs(3)
        );
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        // New TUI conversations default to `RespectUserSettings` (off).
        view.read(&app, |view, ctx| {
            assert_eq!(
                view.conversation_selection
                    .as_ref(ctx)
                    .pending_query_autoexecute_override(ctx),
                AIConversationAutoexecuteMode::RespectUserSettings
            );
            assert!(view.auto_approve_feedback_conversation_id.is_none());
        });

        // Invoking `/auto-approve` executes the TUI `AutoApprove` arm and toggles
        // the selected conversation on.
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::AUTO_APPROVE, None, ctx);
        });
        view.read(&app, |view, ctx| {
            assert_eq!(
                view.conversation_selection
                    .as_ref(ctx)
                    .pending_query_autoexecute_override(ctx),
                AIConversationAutoexecuteMode::RunToCompletion
            );
            assert_eq!(
                view.auto_approve_feedback_conversation_id,
                view.conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx)
            );
            assert_eq!(
                view.transient_hint.current(),
                Some((
                    AUTO_APPROVE_ENABLED_HINT,
                    crate::transient_hint::TransientHintTone::Success
                ))
            );
        });

        // Invoking `/auto-approve` again toggles it back off.
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::AUTO_APPROVE, None, ctx);
        });
        view.read(&app, |view, ctx| {
            assert_eq!(
                view.conversation_selection
                    .as_ref(ctx)
                    .pending_query_autoexecute_override(ctx),
                AIConversationAutoexecuteMode::RespectUserSettings
            );
            assert_eq!(
                view.auto_approve_feedback_conversation_id,
                view.conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx)
            );
            assert_eq!(
                view.transient_hint.current(),
                Some((
                    AUTO_APPROVE_DISABLED_HINT,
                    crate::transient_hint::TransientHintTone::Success
                ))
            );
        });
    });
}

/// `/natural-language-detection` flips `ai_autodetection_enabled_internal`, clears the
/// composer, and reports the new state as success feedback.
///
/// The oracle's `nld_slash_command_toggles_and_reports_its_effects` also drains
/// `warpui_core::telemetry::flush_events` for two `AgentMode.ToggleAutoDetectionSetting`
/// payloads. Neither `flush_events` nor `EventPayload` exists in this fork — the telemetry
/// *channel* was removed with the cloud backend (see `DECLINED.md`) — so that half has no
/// API to assert against here. The emitting call
/// (`record_autodetection_toggle_from_slash_command`) is still made by `set_nld_enabled`.
#[test]
fn nld_slash_command_toggles_and_reports_its_effects() {
    App::test((), |mut app| async move {
        let _agent_mode = warp_core::features::FeatureFlag::AgentMode.override_enabled(true);
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("/natural-language-detection", ctx);
            });
            view.execute_tui_slash_command(&slash_commands::NATURAL_LANGUAGE_DETECTION, None, ctx);
        });

        assert!(app.read(|ctx| {
            *AISettings::as_ref(ctx)
                .ai_autodetection_enabled_internal
                .value()
        }));
        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "");
        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, tone)| (text.to_owned(), tone))
            }),
            Some((
                "Natural language detection enabled.".to_owned(),
                crate::transient_hint::TransientHintTone::Success
            ))
        );

        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("/natural-language-detection", ctx);
            });
            view.execute_tui_slash_command(&slash_commands::NATURAL_LANGUAGE_DETECTION, None, ctx);
        });
        futures_lite::future::yield_now().await;

        assert!(!app.read(|ctx| {
            *AISettings::as_ref(ctx)
                .ai_autodetection_enabled_internal
                .value()
        }));
        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "");
        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, tone)| (text.to_owned(), tone))
            }),
            Some((
                "Natural language detection disabled.".to_owned(),
                crate::transient_hint::TransientHintTone::Success
            ))
        );
    });
}

/// Ported from the pinned oracle's
/// `theme_slash_command_accepts_direct_selection_and_rejects_invalid_values`. `/theme` sets
/// `TuiThemeSettings` and applies the resolved `Appearance` theme immediately; an unparseable
/// argument leaves both untouched and shows `THEME_INVALID_ARGUMENT_HINT`. No
/// `TuiHostTerminalBackground` singleton is registered in this test fixture, so `auto`
/// resolves through `background_luminance(None)`'s documented dark fallback -- matching the
/// oracle test, whose harness likewise has no live terminal to probe.
#[test]
fn theme_slash_command_accepts_direct_selection_and_rejects_invalid_values() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let light = "light".to_owned();

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::THEME, Some(&light), ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                TuiTheme::from(Appearance::as_ref(ctx).theme()),
                TuiTheme::Light
            );
            assert_eq!(
                TuiThemeSettings::as_ref(ctx).selected_theme(),
                TuiTheme::Light
            );
        });

        let dark = "dark".to_owned();
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::THEME, Some(&dark), ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                TuiTheme::from(Appearance::as_ref(ctx).theme()),
                TuiTheme::Dark
            );
            assert_eq!(
                TuiThemeSettings::as_ref(ctx).selected_theme(),
                TuiTheme::Dark
            );
        });

        let auto = "auto".to_owned();
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::THEME, Some(&auto), ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                TuiTheme::from(Appearance::as_ref(ctx).theme()),
                TuiTheme::Dark
            );
            assert_eq!(
                TuiThemeSettings::as_ref(ctx).selected_theme(),
                TuiTheme::Auto
            );
        });

        let invalid = "sepia".to_owned();
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::THEME, Some(&invalid), ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                TuiThemeSettings::as_ref(ctx).selected_theme(),
                TuiTheme::Auto
            );
        });
        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, _)| text.to_owned())
            }),
            Some(THEME_INVALID_ARGUMENT_HINT.to_owned())
        );
    });
}

/// Ported from the pinned oracle's `theme_slash_command_rejects_a_missing_argument`.
#[test]
fn theme_slash_command_rejects_a_missing_argument() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        app.read(|ctx| {
            assert_eq!(
                TuiTheme::from(Appearance::as_ref(ctx).theme()),
                TuiTheme::Dark
            );
            assert_eq!(
                TuiThemeSettings::as_ref(ctx).selected_theme(),
                TuiTheme::Auto
            );
        });

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::THEME, None, ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                TuiTheme::from(Appearance::as_ref(ctx).theme()),
                TuiTheme::Dark
            );
            assert_eq!(
                TuiThemeSettings::as_ref(ctx).selected_theme(),
                TuiTheme::Auto
            );
        });
        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, _)| text.to_owned())
            }),
            Some(THEME_INVALID_ARGUMENT_HINT.to_owned())
        );
    });
}

#[test]
fn auto_approve_actions_control_visible_feedback() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &TuiTerminalSessionAction::ToggleAutoApprove {
                    show_feedback: true,
                },
                ctx,
            );
        });
        view.read(&app, |view, ctx| {
            assert_eq!(
                view.conversation_selection
                    .as_ref(ctx)
                    .pending_query_autoexecute_override(ctx),
                AIConversationAutoexecuteMode::RunToCompletion
            );
            assert_eq!(
                view.auto_approve_feedback_conversation_id,
                view.conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx)
            );
            assert_eq!(
                view.transient_hint.current(),
                Some((
                    AUTO_APPROVE_ENABLED_HINT,
                    crate::transient_hint::TransientHintTone::Success
                ))
            );
        });

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &TuiTerminalSessionAction::ToggleAutoApprove {
                    show_feedback: false,
                },
                ctx,
            );
        });
        view.read(&app, |view, ctx| {
            assert_eq!(
                view.conversation_selection
                    .as_ref(ctx)
                    .pending_query_autoexecute_override(ctx),
                AIConversationAutoexecuteMode::RespectUserSettings
            );
            assert!(view.auto_approve_feedback_conversation_id.is_none());
            assert!(view.auto_approve_feedback_timer.is_none());
        });
    });
}
#[test]
fn footer_model_label_is_a_bounded_click_target() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        // Force the bootstrap (Disabled) state so the footer — and its
        // clickable model label — render deterministically.
        view.update(&mut app, |view, ctx| {
            view.terminal_model.lock().block_list_mut().reinit_shell();
            // A fresh session's input defaults to `InputType::Shell`, and
            // `render_footer` only emits the model label when `!shell_mode`
            // (`FooterSegment::Model`). Switch to AI input mode first — the
            // same way a real conversation entry would — so the clickable
            // model label actually renders.
            view.ai_input_model.update(ctx, |input_model, ctx| {
                input_model.set_input_type(InputType::AI, ctx);
            });
        });

        let model_name = view.read(&app, |view, ctx| {
            LLMPreferences::as_ref(ctx)
                .get_active_base_model(ctx, Some(view.terminal_surface_id))
                .display_name
                .clone()
        });
        let (mut element, scene, buffer) = render_retained_session(&app, &view, 80, 40);
        let (label_col, label_row) = footer_label_position(&buffer, &model_name);
        let inside = (label_col + 1, label_row);
        let outside = (0, label_row);

        assert!(!view.read(&app, |v, _| {
            v.model_label_hover.lock().unwrap().is_hovered()
        }));
        // Hovering onto the label marks the retained handle as hovered.
        dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &mouse_moved(inside.0, inside.1),
        );
        assert!(view.read(&app, |v, _| {
            v.model_label_hover.lock().unwrap().is_hovered()
        }));
        // Hovering back off (into the left footer slot) clears it.
        dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &mouse_moved(outside.0, outside.1),
        );
        assert!(!view.read(&app, |v, _| {
            v.model_label_hover.lock().unwrap().is_hovered()
        }));

        // A press inside the label arms the pending click and is consumed.
        assert!(dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &left_mouse_down(inside.0, inside.1)
        ));
        assert!(view.read(&app, |v, _| {
            v.model_label_hover.lock().unwrap().is_clicked()
        }));
        // Releasing inside disarms (the click handler dispatches ToggleModelMenu).
        assert!(dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &left_mouse_up(inside.0, inside.1)
        ));
        assert!(!view.read(&app, |v, _| {
            v.model_label_hover.lock().unwrap().is_clicked()
        }));

        // A press outside the label does not arm and is not consumed.
        assert!(!dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &left_mouse_down(outside.0, outside.1)
        ));
        assert!(!view.read(&app, |v, _| {
            v.model_label_hover.lock().unwrap().is_clicked()
        }));
        // A following release outside does not fire a click.
        assert!(!dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &left_mouse_up(outside.0, outside.1)
        ));
    });
}

#[test]
fn stale_user_pty_bytes_are_dropped_after_agent_takes_control_or_is_tagged_in() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let writes = Rc::new(RefCell::new(Vec::new()));
        let writes_for_events = writes.clone();
        app.update(|ctx| {
            ctx.subscribe_to_view(&view, move |_, event, _| {
                if let TuiTerminalSessionEvent::WriteUserInput(bytes) = event {
                    writes_for_events.borrow_mut().push(bytes.to_vec());
                }
            });
        });

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &TuiTerminalSessionAction::ForwardUserPtyBytes(b"user".to_vec()),
                ctx,
            );
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.simulate_long_running_block("vim", "");
            terminal_model
                .block_list_mut()
                .active_block_mut()
                .set_is_agent_tagged_in(true);
            drop(terminal_model);
            view.handle_action(
                &TuiTerminalSessionAction::ForwardUserPtyBytes(b"tagged".to_vec()),
                ctx,
            );
            let mut terminal_model = view.terminal_model.lock();
            let conversation_id = AIConversationId::new();
            let task_id = TaskId::new("stale-pty-write".to_owned());
            terminal_model
                .block_list_mut()
                .active_block_mut()
                .set_agent_interaction_mode_for_requested_command(
                    AIAgentActionId::from("stale-pty-command".to_owned()),
                    Some(task_id.clone()),
                    conversation_id,
                );
            terminal_model
                .block_list_mut()
                .active_block_mut()
                .set_agent_interaction_mode_for_agent_monitored_command(&task_id, conversation_id)
                .expect("command should become agent monitored");
            drop(terminal_model);
            view.handle_action(
                &TuiTerminalSessionAction::ForwardUserPtyBytes(b"agent".to_vec()),
                ctx,
            );
        });

        assert_eq!(*writes.borrow(), vec![b"user".to_vec()]);
    });
}

fn focus_test_fixture(app: &mut App) -> FocusTestFixture {
    register_tui_session_view_test_singletons(app);
    app.update(|ctx| add_test_semantic_selection(ctx));
    app.update(TuiAutoupdater::register);
    // Removing or rewinding a conversation runs the session view's history-event
    // subscriptions, which reach the revert registry.
    app.update(crate::tui_revert_registry::TuiFileEditRevertRegistry::register);
    let (window_id, _) = app.update(|ctx| {
        ctx.add_tui_window(
            AddWindowOptions {
                window_style: WindowStyle::NotStealFocus,
                ..Default::default()
            },
            |_| RootTuiView::new(),
        )
    });
    let sessions = app.add_singleton_model(|_| TuiSessions::new_for_test());
    let orchestration = app.update(TuiOrchestrationModel::register);
    app.update(|ctx| TuiSessions::wire_orchestration(&sessions, &orchestration, ctx));
    FocusTestFixture {
        window_id,
        sessions,
    }
}

fn add_focus_test_session(
    app: &mut App,
    fixture: &FocusTestFixture,
    focus: bool,
) -> (ViewHandle<super::TuiTerminalSessionView>, TuiSessionId) {
    let (view, manager) = add_test_terminal_session(app, fixture.window_id);
    let session_id = app.update(|ctx| {
        TuiSessions::register_session(&fixture.sessions, view.clone(), manager, focus, ctx)
    });
    (view, session_id)
}

fn add_focus_test_session_with_settings_file_error(
    app: &mut App,
    fixture: &FocusTestFixture,
    error: SettingsFileError,
) -> ViewHandle<super::TuiTerminalSessionView> {
    let (view, manager) =
        add_test_terminal_session_with_settings_file_error(app, fixture.window_id, Some(error));
    app.update(|ctx| {
        TuiSessions::register_session(&fixture.sessions, view.clone(), manager, true, ctx);
    });
    view
}

fn render_element(element: Box<dyn TuiElement>, ctx: &AppContext, width: u16) -> TuiBuffer {
    render_element_with_size(element, ctx, width, 1)
}

fn render_element_with_size(
    mut element: Box<dyn TuiElement>,
    ctx: &AppContext,
    width: u16,
    height: u16,
) -> TuiBuffer {
    let mut rendered_views = EntityIdMap::default();
    let mut layout_ctx = TuiLayoutContext {
        rendered_views: &mut rendered_views,
    };
    let size = element.layout(
        TuiConstraint::loose(TuiSize::new(width, height)),
        &mut layout_ctx,
        ctx,
    );
    let area = TuiRect::new(0, 0, size.width, size.height);
    let mut buffer = TuiBuffer::empty(area);
    let mut paint_ctx = TuiPaintContext::new(&mut rendered_views);
    {
        let mut surface = TuiPaintSurface::new(&mut buffer);
        element.render(
            TuiScreenPosition::new(i32::from(area.x), i32::from(area.y)),
            &mut surface,
            &mut paint_ctx,
        );
    }
    buffer
}
fn render_session(
    app: &mut App,
    view: &ViewHandle<super::TuiTerminalSessionView>,
    width: u16,
    height: u16,
) -> Vec<String> {
    let mut presenter = TuiPresenter::new();
    app.update(|ctx| {
        let mut invalidation = WindowInvalidation::default();
        invalidation.updated.insert(view.id());
        invalidation
            .updated
            .extend(view.as_ref(ctx).child_view_ids(ctx));
        presenter.invalidate(&invalidation, ctx, view.window_id(ctx));
        presenter
            .present(ctx, view, TuiRect::new(0, 0, width, height))
            .buffer
            .to_lines()
    })
}

/// Like [`render_session`], but hands back the whole painted frame rather than just its text, so
/// a test can assert on the cursor the terminal is told to place.
fn render_session_frame(
    app: &mut App,
    view: &ViewHandle<super::TuiTerminalSessionView>,
    width: u16,
    height: u16,
) -> TuiFrame {
    let mut presenter = TuiPresenter::new();
    app.update(|ctx| {
        let mut invalidation = WindowInvalidation::default();
        invalidation.updated.insert(view.id());
        invalidation
            .updated
            .extend(view.as_ref(ctx).child_view_ids(ctx));
        presenter.invalidate(&invalidation, ctx, view.window_id(ctx));
        presenter.present(ctx, view, TuiRect::new(0, 0, width, height))
    })
}

/// Like [`render_session`], but keeps the styled cell grid so a test can assert on colors as
/// well as glyphs.
fn render_session_buffer(
    app: &mut App,
    view: &ViewHandle<super::TuiTerminalSessionView>,
    width: u16,
    height: u16,
) -> TuiBuffer {
    render_session_frame(app, view, width, height).buffer
}

/// Column of the first non-blank cell in `line`. Used to compare where separately rendered
/// surfaces begin, which is what "aligned to the input's outer edge" means in the buffer.
fn first_visible_column(line: &str) -> usize {
    line.chars()
        .position(|character| !character.is_whitespace())
        .unwrap_or_else(|| panic!("line must contain visible content: {line:?}"))
}

/// Every one of the six editor rows the composer is sized for must actually
/// render: the input box's `TuiConstrainedBox` budget has to cover the border
/// rows *and* the padding row inside each border
/// (`MAX_INPUT_TEXT_ROWS + BORDERED_INPUT_CHROME_ROWS`), otherwise the last
/// rows scroll out of view. Ported from the pin's
/// `input_area_renders_all_six_editor_rows`.
#[test]
fn input_area_renders_all_six_editor_rows() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("input-0\ninput-1\ninput-2\ninput-3\ninput-4\ninput-5", ctx);
            });
        });

        let rendered = render_session(&mut app, &view, 80, 24).join("\n");
        for row in 0..6 {
            assert!(
                rendered.contains(&format!("input-{row}")),
                "input row {row} should be visible:\n{rendered}"
            );
        }
    });
}

fn input_text(view: &ViewHandle<super::TuiTerminalSessionView>, ctx: &AppContext) -> String {
    view.as_ref(ctx)
        .input_view
        .as_ref(ctx)
        .model()
        .as_ref(ctx)
        .content()
        .as_ref(ctx)
        .text()
        .into_string()
}

// `refocus_input_after_question` is the guarded refocus called when a
// question-type blocker finishes (see the `action_model` subscription in
// `TuiTerminalSessionView::new`). These tests call it directly rather than
// driving a real `AskUserQuestion` action through the full blocklist
// preprocess/execute pipeline: that pipeline is asynchronous and, in this
// test environment, does not reliably resolve to a terminal `FinishedAction`
// event (also observable via `queue_tui_permission_action` not reliably
// reaching `Blocked` in other views' tests), which made an end-to-end
// version of this test hang forever awaiting an event that never fires.
// Calling the guarded method directly exercises exactly the logic finding #8
// fixes, deterministically.
#[test]
fn finished_question_does_not_steal_focus_from_a_newer_blocker() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        // Simulate a second blocker that was already created and claimed
        // focus for itself in the same tick, before the question-finished
        // handler runs. `active_blocker_view_id` is the same bookkeeping
        // `sync_blocker_focus` updates when a real blocker takes focus.
        view.update(&mut app, |view, ctx| {
            view.active_blocker_view_id = Some(view.attachment_bar.id());
            ctx.focus(&view.attachment_bar);
            view.refocus_input_after_question(ctx);
        });

        app.read(|ctx| {
            assert!(
                !view.as_ref(ctx).input_view.is_focused(ctx),
                "answering a question must not steal focus from a newer blocker"
            );
            assert!(
                view.as_ref(ctx).attachment_bar.is_focused(ctx),
                "the newer blocker should keep focus"
            );
        });
    });
}

#[test]
fn finished_question_refocuses_input_when_no_other_blocker_is_active() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        // No other blocker is active (`active_blocker_view_id` starts `None`),
        // so the guard must let the refocus through, matching pre-fix
        // behavior for the common case.
        view.update(&mut app, |view, ctx| {
            view.refocus_input_after_question(ctx);
        });

        app.read(|ctx| {
            assert!(
                view.as_ref(ctx).input_view.is_focused(ctx),
                "answering a question with no other blocker queued should still refocus the input"
            );
        });
    });
}

#[test]
fn typeahead_event_inserts_and_overwrites_the_tui_input() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            {
                let mut model = view.terminal_model.lock();
                model.simulate_long_running_block("sleep 5", "");
                model.finish_block();
                model.input_buffer(InputBufferValue {
                    buffer: "ec".to_owned(),
                    session_id: None,
                });
            }
            view.handle_typeahead_event(ctx);
        });
        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "ec");

        view.update(&mut app, |view, ctx| {
            view.terminal_model.lock().input_buffer(InputBufferValue {
                buffer: "echo hi".to_owned(),
                session_id: None,
            });
            view.handle_typeahead_event(ctx);
        });
        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "echo hi");
    });
}

#[test]
fn empty_typeahead_event_leaves_the_tui_input_unchanged() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("draft", ctx);
            });
            {
                let mut model = view.terminal_model.lock();
                model.simulate_long_running_block("sleep 5", "");
                model.finish_block();
            }
            view.handle_typeahead_event(ctx);
        });

        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "draft");
    });
}

#[test]
fn bootstrap_renders_starting_shell_above_input() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, _| {
            view.terminal_model.lock().block_list_mut().reinit_shell();
        });

        let lines = render_session(&mut app, &view, 80, 40);
        let status_index = lines
            .iter()
            .position(|line| line.trim() == "Starting shell...")
            .unwrap_or_else(|| panic!("bootstrap status should render:\n{}", lines.join("\n")));
        let input_index = lines
            .iter()
            .enumerate()
            .skip(status_index + 1)
            .find(|(_, line)| line.contains('▏') || line.contains('▁') || line.contains('─'))
            .map(|(index, _)| index)
            .expect("bootstrap input border should render below the status");
        assert!(status_index < input_index);
    });
}

/// The input child's rendered element is cached by the presenter, and
/// transcript emptiness can flip without any input-owned event (a terminal
/// block landing via the PTY wakeup path only invalidates the session view).
/// The placeholder hint must still switch off the zero-state copy because the
/// provider re-resolves on every layout pass.
#[test]
fn agent_hint_tracks_transcript_emptiness_without_input_invalidation() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        // The agent-mode input hints (`← for conversations` / `Ask the agent
        // anything`) only render when the input is not in shell mode; a fresh
        // session defaults to `InputType::Shell` (which shows `SHELL_HINT`).
        // Switch to AI input mode the same way a real conversation entry would.
        view.update(&mut app, |view, ctx| {
            view.ai_input_model.update(ctx, |input_model, ctx| {
                input_model.set_input_type(InputType::AI, ctx);
            });
        });
        let mut presenter = TuiPresenter::new();

        // Initial full present: every child renders once and is cached.
        let lines = app.update(|ctx| {
            let mut invalidation = WindowInvalidation::default();
            invalidation.updated.insert(view.id());
            invalidation
                .updated
                .extend(view.as_ref(ctx).child_view_ids(ctx));
            presenter.invalidate(&invalidation, ctx, view.window_id(ctx));
            presenter
                .present(ctx, &view, TuiRect::new(0, 0, 100, 40))
                .buffer
                .to_lines()
        });
        assert!(
            lines
                .iter()
                .any(|line| line.contains("← for conversations")),
            "zero state should show the zero-state hint:\n{}",
            lines.join("\n")
        );

        // A finished terminal block lands without any input-owned event; only
        // the session view is invalidated, mirroring the PTY wakeup path.
        view.update(&mut app, |view, _| {
            let mut model = view.terminal_model.lock();
            model
                .block_list_mut()
                .set_agent_view_state(AgentViewState::Inactive);
            model.simulate_block("echo hi", "hi\r\n");
        });
        let lines = app.update(|ctx| {
            let mut invalidation = WindowInvalidation::default();
            invalidation.updated.insert(view.id());
            presenter.invalidate(&invalidation, ctx, view.window_id(ctx));
            presenter
                .present(ctx, &view, TuiRect::new(0, 0, 100, 40))
                .buffer
                .to_lines()
        });
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("← for conversations")),
            "the cached input element must drop the zero-state hint:\n{}",
            lines.join("\n")
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Ask the agent anything")),
            "the started-conversation hint should render:\n{}",
            lines.join("\n")
        );
    });
}

#[test]
fn submit_is_blocked_during_bootstrap_and_allowed_at_prompt() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("draft", ctx);
            });
            view.terminal_model.lock().block_list_mut().reinit_shell();
            view.handle_submitted("draft".to_owned(), ctx);
        });

        assert_eq!(
            app.read(|ctx| input_text(&view, ctx)),
            "draft",
            "bootstrap submission must leave the draft untouched"
        );
        assert!(!view.read(&app, |view, _| {
            view.input_target().agent_editor_owns_input()
        }));
        assert!(TuiInputTarget::AgentEditor.agent_editor_owns_input());
    });
}

#[test]
fn long_running_command_keeps_input_hidden() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, _| {
            view.terminal_model
                .lock()
                .simulate_long_running_block("cat", "");
        });

        let lines = render_session(&mut app, &view, 80, 40);
        assert!(
            !lines
                .iter()
                .any(|line| line.trim_end() == "Starting shell..."),
            "LRC must not render bootstrap status:\n{}",
            lines.join("\n")
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.chars().any(|glyph| "┌┐└┘─│▁▏▕▔".contains(glyph))),
            "LRC must keep the input editor hidden:\n{}",
            lines.join("\n")
        );
        // Manual attachment renders as a ghosted row in the input's slot
        // while the command owns input. Ported from the pin's
        // `long_running_command_keeps_input_hidden` (`02b53fcd8`) -- the fork
        // previously asserted a fixed "ctrl-c to interrupt" string here; that
        // was superseded by the pin's keybinding-driven attach hint (this
        // file's `running_command_hint`/`input_hints::long_running_command_hint`).
        let hint = view.read(&app, |view, ctx| {
            view.running_command_hint(ctx)
                .expect("visible running command should have an attachment hint")
        });
        assert!(
            lines.iter().any(|line| line.trim() == hint),
            "LRC must render the attach hint row:\n{}",
            lines.join("\n")
        );
        assert_eq!(hint, "Ctrl + Shift + \u{23ce}  to use agent");
        assert!(
            lines
                .iter()
                .all(|line| !line.contains(RUNNING_COMMAND_DETACH_HINT)),
            "LRC must not show the detach hint before agent attachment:\n{}",
            lines.join("\n")
        );
    });
}

/// Visible startup-script execution also routes input to the PTY, but it is
/// not a user-controlled command: the attach hint row must not appear.
#[test]
fn visible_startup_script_shows_no_interrupt_hint() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            {
                let mut terminal_model = view.terminal_model.lock();
                terminal_model.block_list_mut().reinit_shell();
                terminal_model
                    .update_blockheight_items(TRANSCRIPT_BLOCK_SPACING.block_padding, 0.0);
                // Advance past WarpInput, then leave an unfinished startup-script
                // block with visible output owning PTY input.
                terminal_model.simulate_block("bootstrap", "");
                terminal_model.simulate_long_running_block("shell init", "startup output\r\n");
            }
            // Startup-script input is not an attachable user long-running command:
            // the manual-attach machinery must stay fully inert while it owns the PTY.
            // Ported from the pin's `visible_startup_script_shows_no_running_command_hint`
            // (`02b53fcd8`); these three assertions were dropped when this test was first
            // ported even though every symbol they need already exists here.
            assert!(
                !view
                    .session_state
                    .resolve(ctx)
                    .expect("session state resolves")
                    .can_attach_agent_to_running_command()
            );
            assert!(
                !view
                    .keymap_context(ctx)
                    .set
                    .contains(SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG)
            );
            assert!(
                !view.try_attach_agent_to_running_command(ctx),
                "startup-script input is not an attachable user LRC"
            );
        });
        assert!(
            view.read(&app, |view, _| view.input_target().pty_owns_input()),
            "fixture should route input to the PTY during the visible startup script"
        );

        let lines = render_session(&mut app, &view, 80, 40);
        assert!(
            !lines.iter().any(|line| line.contains("to use agent")),
            "startup-script execution must not advertise agent attachment:\n{}",
            lines.join("\n")
        );
    });
}

/// Ported from the pin's `zero_state_running_command_hint_shows_attachment`
/// (`02b53fcd8`). A hidden long-running command's output stays out of the
/// transcript (`should_hide_command_grid`, used for shell-init/startup-style
/// commands), but the manual-attach hint still renders alongside the zero
/// state so the user can reach the agent even before any visible output.
#[test]
fn zero_state_running_command_hint_shows_attachment() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.simulate_long_running_block("cat", "");
            terminal_model
                .block_list_mut()
                .active_block_mut()
                .set_should_hide_command_grid(true);
            drop(terminal_model);

            assert!(
                view.transcript.as_ref(ctx).is_empty(),
                "hidden command without output should preserve zero state"
            );
            assert!(
                view.session_state
                    .resolve(ctx)
                    .expect("session state resolves")
                    .user_owns_running_command()
            );
        });

        let lines = render_session(&mut app, &view, 80, 40);
        assert!(
            lines.iter().any(|line| line.contains("Phosphor Agent")),
            "zero state should remain visible:\n{}",
            lines.join("\n")
        );
        assert!(
            lines.iter().any(|line| line.contains("to use agent")),
            "zero state should preserve manual attachment:\n{}",
            lines.join("\n")
        );
    });
}

#[test]
fn zero_state_renders_with_only_zero_height_bootstrap_blocks() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, _| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.block_list_mut().reinit_shell();
            terminal_model.update_blockheight_items(TRANSCRIPT_BLOCK_SPACING.block_padding, 0.0);
            terminal_model.simulate_block("bootstrap", "");
            terminal_model.simulate_long_running_block("shell init", "");
            let bootstrap_block_id = terminal_model.block_list().active_block().id().clone();
            terminal_model.finish_block();
            let bootstrap_block = terminal_model
                .block_list_mut()
                .mut_block_from_id(&bootstrap_block_id)
                .expect("bootstrap block should remain in the block list");
            bootstrap_block.set_should_hide_command_grid(true);
            terminal_model.update_blockheight_items(
                BlockPadding {
                    bottom: 1.0,
                    ..TRANSCRIPT_BLOCK_SPACING.block_padding
                },
                0.0,
            );

            let block_list = terminal_model.block_list();
            let bootstrap_block = block_list
                .block_with_id(&bootstrap_block_id)
                .expect("bootstrap block should remain in the block list");
            assert!(
                should_render_terminal_block(bootstrap_block, block_list),
                "fixture should contain an eligible shell bootstrap block"
            );
            assert!(
                block_content_rows(bootstrap_block).is_empty(),
                "fixture bootstrap block should have zero displayed height"
            );
        });
        view.read(&app, |view, ctx| {
            assert!(
                view.transcript.as_ref(ctx).is_empty(),
                "zero-height terminal blocks should leave the transcript empty"
            );
        });

        let mut presenter = TuiPresenter::new();
        let frame = app.update(|ctx| {
            let mut invalidation = WindowInvalidation::default();
            invalidation.updated.insert(view.id());
            invalidation
                .updated
                .extend(view.as_ref(ctx).child_view_ids(ctx));
            presenter.invalidate(&invalidation, ctx, fixture.window_id);
            presenter.present(ctx, &view, TuiRect::new(0, 0, 200, 40))
        });
        let lines = frame.buffer.to_lines();
        let title_row = lines
            .iter()
            .position(|line| line.contains("Phosphor Agent"))
            .expect("zero state should render the Phosphor Agent title");
        assert!(
            title_row < 28,
            "zero-state title should render in the transcript area:\n{}",
            lines.join("\n")
        );
        // The 32-column animation panel is centered in the 152 columns left
        // after the 48-column copy region: 48 + (152 - 32) / 2 = 108.
        let animation_start = 108;
        let animation_end = 140;
        assert!(
            lines.iter().take(28).any(|line| line
                .chars()
                .skip(animation_start)
                .take(animation_end - animation_start)
                .any(|character| character != ' ')),
            "animation content should render in the centered remaining-space panel:\n{}",
            lines.join("\n")
        );
        assert!(
            lines.iter().take(28).any(|line| line
                .chars()
                .skip(animation_end)
                .any(|character| character != ' ')),
            "starfield content should extend beyond the centered logo panel:\n{}",
            lines.join("\n")
        );
    });
}

#[test]
fn zero_state_transitions_through_bootstrap_lifecycle() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        // Phase 1: an unfinished ScriptExecution block with visible output suppresses the zero
        // state. The `|| !block.finished()` lifecycle guard covers this case: PTY input is still
        // routed to the block, so the zero state must stay hidden while the block runs.
        view.update(&mut app, |view, _| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.block_list_mut().reinit_shell();
            terminal_model.update_blockheight_items(TRANSCRIPT_BLOCK_SPACING.block_padding, 0.0);
            // Advance past WarpInput to ScriptExecution.
            terminal_model.simulate_block("bootstrap", "");
            // Create an unfinished ScriptExecution block with visible output rows.
            terminal_model.simulate_long_running_block("shell init", "startup output\r\n");
        });
        view.read(&app, |view, ctx| {
            assert!(
                !view.transcript.as_ref(ctx).is_empty(),
                "unfinished startup block with visible content should suppress the zero state"
            );
        });

        // Phase 2: once the startup block finishes it no longer satisfies the lifecycle guard
        // (it is finished, not restored, and not PostBootstrapPrecmd), so the zero state returns.
        view.update(&mut app, |view, _| {
            let mut terminal_model = view.terminal_model.lock();
            // Advance bootstrap stage so finish_block() promotes the list to PostBootstrapPrecmd.
            terminal_model.block_list_mut().set_bootstrapped();
            terminal_model.finish_block();
        });
        view.read(&app, |view, ctx| {
            assert!(
                view.transcript.as_ref(ctx).is_empty(),
                "finished ScriptExecution block should no longer suppress the zero state"
            );
        });

        // Phase 3: the first normal post-bootstrap command dismisses the zero state.
        view.update(&mut app, |view, _| {
            view.terminal_model
                .lock()
                .simulate_block("echo hello", "hello\r\n");
        });
        view.read(&app, |view, ctx| {
            assert!(
                !view.transcript.as_ref(ctx).is_empty(),
                "post-bootstrap command with visible output should dismiss the zero state"
            );
        });
    });
}

fn render_footer_lines(
    app: &mut App,
    view: &ViewHandle<super::TuiTerminalSessionView>,
    width: u16,
) -> Vec<String> {
    app.update(|ctx| {
        let footer = view.as_ref(ctx).render_footer(ctx).finish();
        render_element(footer, ctx, width).to_lines()
    })
}

/// A replacing hint occupies the whole status row, so no section separators,
/// branch arrows, or usage text should appear alongside it.
fn assert_footer_segments_absent(lines: &[String]) {
    let row = lines.join("\n");
    assert!(
        !row.contains('│'),
        "a replacing hint should occupy the whole row with no sections: {row}"
    );
    assert!(
        !row.contains(" ↬ "),
        "the cwd/branch section is absent: {row}"
    );
    assert!(
        !row.contains("credits"),
        "the usage section is absent: {row}"
    );
}

#[test]
fn new_slash_command_clears_shell_commands_from_transcript() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, _| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.block_list_mut().set_bootstrapped();
            terminal_model.simulate_block("echo before-new", "before-new\r\n");
        });

        view.read(&app, |view, ctx| {
            assert!(!view.transcript.as_ref(ctx).is_empty());
            assert!(
                view.terminal_model
                    .lock()
                    .block_list()
                    .blocks()
                    .iter()
                    .any(|block| block.command_to_string() == "echo before-new")
            );
        });

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::NEW, None, ctx);
        });

        view.read(&app, |view, ctx| {
            assert!(
                view.transcript.as_ref(ctx).is_empty(),
                "/new should clear both agent and shell transcript blocks"
            );
            assert_eq!(
                view.terminal_model.lock().block_list().blocks().len(),
                1,
                "/new should leave only the active prompt block"
            );
        });
    });
}

/// `/clear` is a TUI-only alias for `/agent`/`/new` (pinned oracle
/// `commands_tests.rs::clear_command_has_correct_registry_metadata`: "Clear the transcript and
/// start a new conversation (alias for /agent)"). Mirrors
/// `new_slash_command_clears_shell_commands_from_transcript` above to confirm `/clear` reaches
/// the same `SlashCommandKind::Agent | SlashCommandKind::New | SlashCommandKind::Clear` handler.
#[test]
fn clear_slash_command_clears_shell_commands_from_transcript() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, _| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.block_list_mut().set_bootstrapped();
            terminal_model.simulate_block("echo before-clear", "before-clear\r\n");
        });

        view.read(&app, |view, ctx| {
            assert!(!view.transcript.as_ref(ctx).is_empty());
            assert!(
                view.terminal_model
                    .lock()
                    .block_list()
                    .blocks()
                    .iter()
                    .any(|block| block.command_to_string() == "echo before-clear")
            );
        });

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::CLEAR, None, ctx);
        });

        view.read(&app, |view, ctx| {
            assert!(
                view.transcript.as_ref(ctx).is_empty(),
                "/clear should clear both agent and shell transcript blocks"
            );
            assert_eq!(
                view.terminal_model.lock().block_list().blocks().len(),
                1,
                "/clear should leave only the active prompt block"
            );
        });
    });
}

/// The `/exit` and `/view-logs` slash commands must be registered and TUI-executable (#338).
/// Their dispatch arms already existed and are unchanged by this port -- `SlashCommandKind::Exit`
/// terminates the process and `SlashCommandKind::ViewLogs` spawns a real filesystem zip + reveal
/// in the file manager, neither of which this view-test harness can exercise end-to-end -- so
/// this proves what #338 was actually blocked on: reachability from the `/` menu.
#[test]
fn exit_and_view_logs_slash_commands_are_registered_and_tui_executable() {
    for command in [&*slash_commands::EXIT, &*slash_commands::VIEW_LOGS] {
        assert!(
            slash_commands::COMMAND_REGISTRY
                .get_command_with_name(command.name)
                .is_some(),
            "{} must be registered",
            command.name
        );
        assert!(
            command.supports_tui(),
            "{} must be executable in the TUI",
            command.name
        );
    }
}

/// The `/mcp` slash command must be registered, TUI-executable, and open the MCP menu on
/// execution -- the dispatch arm already existed (`SlashCommandKind::Mcp` in
/// `execute_tui_slash_command`), but nothing produced it before `commands::MCP` was
/// registered (#338).
#[test]
fn mcp_slash_command_opens_the_mcp_menu() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        assert!(slash_commands::MCP.supports_tui());

        view.read(&app, |view, ctx| {
            assert!(
                !view.mcp_menu.as_ref(ctx).is_open(ctx),
                "MCP menu should start closed"
            );
        });
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::MCP, None, ctx);
        });
        view.read(&app, |view, ctx| {
            assert!(
                view.mcp_menu.as_ref(ctx).is_open(ctx),
                "/mcp should open the MCP menu"
            );
        });
    });
}
#[test]
fn footer_renders_agent_sections_left_aligned() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let usage = render_context_usage_entry(0.18, ctx);
            let row = render_status_footer_row(
                FooterSegments {
                    ordered: vec![
                        FooterSegment::Model(
                            TuiText::new("TestModel")
                                .with_style(builder.primary_text_style())
                                .truncate()
                                .finish(),
                        ),
                        FooterSegment::WorkingDirectory("/home/user/warp".to_owned()),
                        FooterSegment::GitBranch("main".to_owned()),
                        FooterSegment::ContextWindowUsage(usage),
                        FooterSegment::GitDiff {
                            files_changed: 2,
                            additions: 3,
                            deletions: 1,
                        },
                    ],
                },
                &builder,
            )
            .finish();
            let lines = render_element(row, ctx, 120).to_lines();
            let line = lines.join("\n");

            assert_eq!(
                lines,
                vec!["TestModel | /home/user/warp ↬ main | 18% context | ☰ 2 • +3 -1"],
                "agent footer is left-aligned in order model → cwd/branch → usage → diff"
            );
            assert!(
                line.starts_with("TestModel"),
                "the first segment starts at the left edge (no flex-spacer padding)"
            );
            assert!(!line.contains('←'), "the conversations callout is absent");
        });
    });
}

#[test]
fn footer_does_not_render_credit_actions() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        let lines = render_session(&mut app, &view, 80, 40);
        assert!(
            lines.iter().all(|line| {
                !line.contains("Out of credits")
                    && !line.contains("Compare plans")
                    && !line.contains("Use your own API keys")
            }),
            "credit actions belong to the failed transcript block:\n{}",
            lines.join("\n")
        );
    });
}

// The pin's `figma_statusline_metadata_formats_are_stable`. Its
// `format_todo_progress` assertions were dropped when this test was first
// ported (fix(#576) 1fce977cd) because that function did not exist in the
// fork yet; they are restored now that `AgentTodoList` is ported.
#[test]
fn statusline_datetime_formats_are_stable() {
    let now = NaiveDate::from_ymd_opt(2026, 7, 20)
        .unwrap()
        .and_hms_opt(13, 8, 0)
        .unwrap();
    assert_eq!(format_statusline_date(now), "July 20, 2026");
    assert_eq!(format_statusline_time_12_hour(now), "1:08pm");
    assert_eq!(format_statusline_time_24_hour(now), "13:08");
    assert_eq!(format_todo_progress(1, 10, false), "❒ 1/10");
    assert_eq!(format_todo_progress(10, 10, true), "✓ 10/10");
}

#[test]
fn statusline_datetime_requests_a_periodic_repaint() {
    App::test((), |app| async move {
        app.read(|ctx| {
            let datetime =
                render_statusline_datetime(format_statusline_time_24_hour, TuiStyle::default());
            let mut presenter = TuiPresenter::new();
            let frame = presenter.present_element(datetime, TuiRect::new(0, 0, 5, 1), ctx);
            assert!(
                frame.repaint_at.is_some(),
                "visible date/time items must repaint so their value cannot freeze"
            );
        });
    });
}

#[test]
fn footer_renders_bash_sections_without_model_or_usage() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            // Model and usage are omitted entirely rather than passed and
            // ignored: `render_footer` (the only real caller) never resolves
            // those items while in shell mode, so this reflects the segments
            // it would actually hand to `render_status_footer_row`.
            let row = render_status_footer_row(
                FooterSegments {
                    ordered: vec![
                        FooterSegment::ShellMode,
                        FooterSegment::WorkingDirectory("/home/user/warp".to_owned()),
                        FooterSegment::GitBranch("main".to_owned()),
                        FooterSegment::GitDiff {
                            files_changed: 2,
                            additions: 3,
                            deletions: 1,
                        },
                    ],
                },
                &builder,
            )
            .finish();
            let buffer = render_element(row, ctx, 120);
            assert_eq!(
                buffer[(0, 0)].fg,
                builder
                    .shell_command_accent_style()
                    .fg
                    .expect("shell command accent has a foreground")
            );
            let lines = buffer.to_lines();
            let line = lines.join("\n");

            assert_eq!(
                lines,
                vec![format!("{SHELL_MODE_HINT} /home/user/warp ↬ main | ☰ 2 • +3 -1")],
                "bash footer leads with the shell-mode indicator and hides model/usage"
            );
            assert!(
                line.starts_with(SHELL_MODE_HINT),
                "shell mode is the first segment"
            );
            assert!(
                !line.contains("TestModel"),
                "model segment is hidden in bash mode"
            );
            assert!(
                !line.contains("18% context"),
                "usage segment is hidden in bash mode"
            );
        });
    });
}

#[test]
fn footer_transient_state_replaces_all_sections() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        // ctrl-c exit confirmation replaces the whole row.
        view.update(&mut app, |view, _| {
            view.exit_confirmation.arm(Instant::now());
        });
        let lines = render_footer_lines(&mut app, &view, 80);
        assert_eq!(lines, vec![CTRL_C_EXIT_HINT]);
        assert_footer_segments_absent(&lines);

        // Loading-conversation hint replaces the whole row.
        view.update(&mut app, |view, _| {
            view.exit_confirmation.disarm();
            view.conversation_restore_state = ConversationRestoreState::Loading {
                origin: TuiConversationRestoreOrigin::ConversationList,
                request_id: 0,
                future: None,
            };
        });
        let lines = render_footer_lines(&mut app, &view, 80);
        assert_eq!(lines, vec![LOADING_CONVERSATION_HINT]);
        assert_footer_segments_absent(&lines);

        // A transient notice replaces the whole row.
        view.update(&mut app, |view, ctx| {
            view.conversation_restore_state = ConversationRestoreState::Idle;
            view.show_transient_hint("transient notice".to_owned(), ctx);
        });
        let lines = render_footer_lines(&mut app, &view, 80);
        assert_eq!(lines, vec!["transient notice"]);
        assert_footer_segments_absent(&lines);

        // Priority: when ctrl-c, loading, and a transient notice all overlap,
        // ctrl-c wins (the existing ctrl-c → loading → transient order).
        view.update(&mut app, |view, ctx| {
            view.exit_confirmation.arm(Instant::now());
            view.conversation_restore_state = ConversationRestoreState::Loading {
                origin: TuiConversationRestoreOrigin::ConversationList,
                request_id: 1,
                future: None,
            };
            view.show_transient_hint("transient notice".to_owned(), ctx);
        });
        let lines = render_footer_lines(&mut app, &view, 80);
        assert_eq!(lines, vec![CTRL_C_EXIT_HINT]);
    });
}

/// Ported from Warp's `crates/warp_tui/src/terminal_session_view_tests.rs` at
/// the pinned oracle (`02b53fcd8` — see `ORACLE.md`) as part of #384. A
/// reload failure (e.g. the linked `TuiZeroStateObject::AsciiFile` was
/// deleted or edited into something invalid) surfaces as an error-toned
/// transient footer hint rather than failing silently.
#[test]
fn zero_state_reload_failure_renders_as_an_error_footer_hint() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        app.update(|ctx| {
            ZeroStateAnimationConfig::handle(ctx).update(ctx, |_, ctx| {
                ctx.emit(ZeroStateAnimationConfigEvent::LoadFailed(
                    ZeroStateAnimationLoadFailure::Reload,
                ));
            });
        });

        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, tone)| (text.to_owned(), tone))
            }),
            Some((
                super::ZERO_STATE_ASCII_RELOAD_FAILED_HINT.to_owned(),
                crate::transient_hint::TransientHintTone::Error
            ))
        );

        let lines = render_footer_lines(&mut app, &view, 120);
        assert_eq!(lines, vec![super::ZERO_STATE_ASCII_RELOAD_FAILED_HINT]);
    });
}

/// Ported from the pin's `settings_reload_failure_renders_as_an_error_footer_hint`
/// (upstream `73529d1d6`). A failed settings hot-reload reuses the same
/// transient error slot the zero-state failure above uses; the detailed
/// diagnostics stay in the log.
#[test]
fn settings_reload_failure_renders_as_an_error_footer_hint() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        app.update(|ctx| {
            WarpConfig::handle(ctx).update(ctx, |_, ctx| {
                ctx.emit(WarpConfigUpdateEvent::SettingsErrors(
                    SettingsFileError::InvalidSettings(vec!["Theme".to_owned()]),
                ));
            });
        });

        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, tone)| (text.to_owned(), tone))
            }),
            Some((
                super::SETTINGS_INVALID_VALUES_HINT.to_owned(),
                crate::transient_hint::TransientHintTone::Error
            ))
        );

        let lines = render_footer_lines(&mut app, &view, 120);
        assert_eq!(lines, vec![super::SETTINGS_INVALID_VALUES_HINT]);
    });
}

/// Ported from the pin's `startup_settings_parse_failure_renders_as_an_error_footer_hint`
/// (upstream `73529d1d6`). A settings file that failed to parse at startup is
/// reported by the first session, checked at construction rather than via a
/// later reload event.
#[test]
fn startup_settings_parse_failure_renders_as_an_error_footer_hint() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let view = add_focus_test_session_with_settings_file_error(
            &mut app,
            &fixture,
            SettingsFileError::FileParseFailed("expected a value".to_owned()),
        );

        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, tone)| (text.to_owned(), tone))
            }),
            Some((
                super::SETTINGS_PARSE_FAILED_HINT.to_owned(),
                crate::transient_hint::TransientHintTone::Error
            ))
        );

        let lines = render_footer_lines(&mut app, &view, 120);
        assert_eq!(lines, vec![super::SETTINGS_PARSE_FAILED_HINT]);
    });
}

/// Fork addition (no equivalent at the pin): keys that match no setting are
/// their own `SettingsFileError` variant, so the TUI must not describe them
/// with the "invalid values" hint -- nothing failed to load, the lines are
/// merely inert.
#[test]
fn unrecognized_settings_keys_render_as_their_own_error_footer_hint() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        app.update(|ctx| {
            WarpConfig::handle(ctx).update(ctx, |_, ctx| {
                ctx.emit(WarpConfigUpdateEvent::SettingsErrors(
                    SettingsFileError::UnknownKeys(vec!["cloud_sync".to_owned()]),
                ));
            });
        });

        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, tone)| (text.to_owned(), tone))
            }),
            Some((
                super::SETTINGS_UNKNOWN_KEYS_HINT.to_owned(),
                crate::transient_hint::TransientHintTone::Error
            ))
        );
        assert_ne!(
            super::SETTINGS_UNKNOWN_KEYS_HINT,
            super::SETTINGS_INVALID_VALUES_HINT
        );

        let lines = render_footer_lines(&mut app, &view, 120);
        assert_eq!(lines, vec![super::SETTINGS_UNKNOWN_KEYS_HINT]);
    });
}

/// Ported from Warp's `crates/warp_tui/src/terminal_session_view_tests.rs` at
/// the pinned oracle (`02b53fcd8`) as part of #384. An initial-load failure
/// (a configured `TuiZeroStateObject::AsciiFile` that never resolves, e.g. a
/// missing file) also surfaces as an error-toned hint, checked at session
/// construction rather than via a later `LoadFailed` event.
#[test]
fn zero_state_initial_load_failure_shows_an_error_footer_hint() {
    App::test((), |mut app| async move {
        let temp_dir = TempDir::new().unwrap();
        let config = ZeroStateAnimationConfig::load(
            &TuiZeroStateObject::AsciiFile {
                path: "missing.txt".into(),
            },
            5.0,
            0.18,
            temp_dir.path(),
        );
        app.add_singleton_model(move |_| config);
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, tone)| (text.to_owned(), tone))
            }),
            Some((
                super::ZERO_STATE_ASCII_INITIAL_LOAD_FAILED_HINT.to_owned(),
                crate::transient_hint::TransientHintTone::Error
            ))
        );
    });
}

#[test]
fn footer_conversations_callout_no_longer_renders() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        // The model-led status row only renders in AI input mode (a fresh
        // session defaults to `InputType::Shell`); switch to it the same way
        // a real conversation entry would, so the row replaces any callout.
        view.update(&mut app, |view, ctx| {
            view.ai_input_model.update(ctx, |input_model, ctx| {
                input_model.set_input_type(InputType::AI, ctx);
            });
        });

        // The active-model label leads the row. In this BYOP harness (no
        // provider configured) that is the "add one in Settings" prompt; read
        // it from the source of truth rather than hardcoding a cloud model.
        let model_name = view.read(&app, |view, ctx| {
            LLMPreferences::as_ref(ctx)
                .get_active_base_model(ctx, Some(view.terminal_surface_id))
                .display_name
                .clone()
        });

        // With an empty input and no replacing hint, the footer renders the
        // left-aligned sectioned row — never the obsolete `← for conversations`
        // callout (render_left_footer_hint and the show_conversations_hint
        // branch are removed, not merely unreachable).
        let lines = render_footer_lines(&mut app, &view, 80);
        let row = lines.join("\n");
        assert!(
            !row.contains("← for conversations"),
            "the conversations callout must not render: {row}"
        );
        assert!(
            !row.contains('←'),
            "no conversations-callout glyph remains: {row}"
        );
        // The clickable `▶▶` auto-approve control leads the default statusline
        // (it is the first enabled item and now renders in every non-shell-mode
        // session); the model label follows it.
        assert!(
            row.starts_with(&format!("▶▶ | {model_name} ")),
            "the auto-approve control and model-led status row render in place \
             of the callout: {row}"
        );
    });
}
#[test]
fn interrupt_event_projects_to_high_level_pty_intent() {
    let event = TuiTerminalSessionEvent::InterruptPty;
    assert!(matches!(event.pty_intent(), Some(PtyIntent::Interrupt)));
}

#[test]
fn user_input_event_projects_to_raw_user_bytes() {
    let event = TuiTerminalSessionEvent::WriteUserInput(b"hello\r".to_vec().into());
    let Some(PtyIntent::WriteBytes(bytes)) = event.pty_intent() else {
        panic!("user input event should map to raw PTY bytes");
    };
    assert_eq!(&*bytes, b"hello\r");
}
#[test]
fn plan_toggle_uses_contextual_ctrl_p_and_ctrl_shift_p() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        app.read(|ctx| {
            let toggle = ctx
                .get_binding_by_name(PLAN_TOGGLE_BINDING_NAME)
                .expect("primary plan toggle binding");
            assert_eq!(
                *toggle.trigger,
                Trigger::Keystrokes(vec![Keystroke::parse("ctrl-shift-P").unwrap()])
            );

            let fallback = ctx
                .editable_bindings()
                .find(|binding| binding.name == CONTEXTUAL_PLAN_TOGGLE_BINDING_NAME)
                .expect("contextual plan toggle binding");
            let ctrl_p = Trigger::Keystrokes(vec![Keystroke::parse("ctrl-p").unwrap()]);
            assert_eq!(*fallback.trigger, ctrl_p);

            let mut input_without_plan = Context::default();
            input_without_plan.set.insert("TuiInputView");
            let mut input_with_plan = input_without_plan.clone();
            input_with_plan.set.insert(PLAN_TOGGLE_AVAILABLE_FLAG);
            let mut enhanced_input_with_plan = input_with_plan.clone();
            enhanced_input_with_plan
                .set
                .insert(KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG);
            assert!(!fallback.in_context(&input_without_plan));
            assert!(fallback.in_context(&input_with_plan));
            assert!(!fallback.in_context(&enhanced_input_with_plan));

            let ctrl_p_move_up = ctx
                .editable_bindings()
                .find(|binding| binding.name == "tui:input:move_up" && *binding.trigger == ctrl_p)
                .expect("Ctrl+P move-up fallback");
            assert!(ctrl_p_move_up.in_context(&input_without_plan));
            assert!(!ctrl_p_move_up.in_context(&input_with_plan));
            assert!(ctrl_p_move_up.in_context(&enhanced_input_with_plan));
        });
    });
}

#[test]
fn auto_approve_uses_ctrl_shift_i() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        app.read(|ctx| {
            let binding = ctx
                .editable_bindings()
                .find(|binding| binding.name == AUTO_APPROVE_TOGGLE_BINDING_NAME)
                .expect("auto-approve toggle binding");
            assert_eq!(
                *binding.trigger,
                Trigger::Keystrokes(vec![Keystroke::parse("ctrl-shift-I").unwrap()])
            );

            let mut session_context = Context::default();
            session_context
                .set
                .insert(TuiTerminalSessionView::ui_name());
            assert!(binding.in_context(&session_context));
        });
    });
}
#[test]
fn ctrl_d_is_owned_by_the_session_surface_not_input_delete_forward() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        app.read(|ctx| {
            let ctrl_d = Trigger::Keystrokes(vec![Keystroke::parse("ctrl-d").unwrap()]);

            // The prompt input no longer binds ctrl-d to delete-forward (the
            // session surface owns it); only the `delete` key deletes forward.
            let input_delete_forward_binds_ctrl_d = ctx
                .editable_bindings()
                .any(|b| b.name == "tui:input:delete_forward" && *b.trigger == ctrl_d);
            assert!(
                !input_delete_forward_binds_ctrl_d,
                "input delete-forward must not bind ctrl-d"
            );

            // The generic editor keeps ctrl-d as delete-forward.
            let editor_delete_forward_binds_ctrl_d = ctx
                .editable_bindings()
                .any(|b| b.name == "tui:editor:delete_forward" && *b.trigger == ctrl_d);
            assert!(
                editor_delete_forward_binds_ctrl_d,
                "editor delete-forward should still bind ctrl-d"
            );

            // The session handles ctrl-d only while the prompt is focused.
            // When a process owns focus, ctrl-d falls through to the terminal
            // element's standard PTY key encoding.
            let session_binds_ctrl_d = ctx.get_key_bindings().any(|b| {
                *b.trigger == ctrl_d && b.name.is_empty() && b.group == Some(TUI_BINDING_GROUP)
            });
            assert!(
                session_binds_ctrl_d,
                "the session should bind ctrl-d for prompt exit / deletion"
            );
        });
    });
}

#[test]
fn allow_and_reject_blocked_lrc_actions_are_wired_to_distinct_ctrl_bindings() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        app.read(|ctx| {
            let ctrl_o = Trigger::Keystrokes(vec![Keystroke::parse("ctrl-o").unwrap()]);
            let ctrl_r = Trigger::Keystrokes(vec![Keystroke::parse("ctrl-r").unwrap()]);

            // The keyboard path for "[Allow]" (ctrl-o) and "[Reject]" (ctrl-r)
            // are both registered as fixed, non-remappable session bindings --
            // distinct keys, same binding group as the rest of the session's
            // reserved keys.
            let allow_bound = ctx.get_key_bindings().any(|b| {
                *b.trigger == ctrl_o && b.name.is_empty() && b.group == Some(TUI_BINDING_GROUP)
            });
            assert!(allow_bound, "ctrl-o should allow a blocked agent action");

            let reject_bound = ctx.get_key_bindings().any(|b| {
                *b.trigger == ctrl_r && b.name.is_empty() && b.group == Some(TUI_BINDING_GROUP)
            });
            assert!(reject_bound, "ctrl-r should reject a blocked agent action");

            // Reject must not have been wired onto ctrl-c (the session's
            // take-control / interrupt binding) or reused the allow trigger.
            let ctrl_c = Trigger::Keystrokes(vec![Keystroke::parse("ctrl-c").unwrap()]);
            assert_ne!(ctrl_r, ctrl_c);
            assert_ne!(ctrl_r, ctrl_o);
        });
    });
}

#[test]
fn non_command_prompt_preserves_leading_whitespace() {
    assert_eq!(raw_prompt_if_not_blank("  /compact"), Some("  /compact"));
}

#[test]
fn whitespace_only_prompt_is_ignored() {
    assert_eq!(raw_prompt_if_not_blank(" \t\n"), None);
}

#[test]
fn file_export_success_message_includes_destination_path() {
    let directory = tempfile::tempdir().expect("temp directory");
    let export = export_conversation_markdown(
        Some(directory.path().to_str().expect("UTF-8 temp path")),
        Some("conversation.md"),
        None,
        "# Conversation",
    )
    .expect("conversation export");

    assert_eq!(
        export_file_success_message(&export),
        format!("Conversation exported to {}", export.path().display())
    );
}

#[test]
fn resize_event_maps_to_pty_resize_intent() {
    let last_size = SizeInfo::new_without_font_metrics(24, 120);
    let size_update = SizeUpdate::from_cell_dimensions(last_size, 8, 42);
    let event = TuiTerminalSessionEvent::Resize(size_update);

    let Some(PtyIntent::Resize(actual_update)) = event.pty_intent() else {
        panic!("resize event should map to a PTY resize intent");
    };
    assert_eq!(actual_update.new_size().rows(), 8);
    assert_eq!(actual_update.new_size().columns(), 42);
}

#[test]
fn terminal_wakeup_redraws_only_the_focused_session() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (foreground, _) = add_focus_test_session(&mut app, &fixture, true);
        let (background, _) = add_focus_test_session(&mut app, &fixture, false);

        assert!(foreground.update(&mut app, |view, ctx| { view.handle_terminal_wakeup(ctx) }));
        assert!(!background.update(&mut app, |view, ctx| { view.handle_terminal_wakeup(ctx) }));
    });
}

#[test]
fn terminal_wakeup_focuses_a_new_long_running_command() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let input_id = view.read(&app, |view, _| view.input_view.id());

        view.update(&mut app, |view, ctx| {
            view.terminal_model
                .lock()
                .simulate_long_running_block("cat", "");
            assert!(view.input_target().pty_owns_input());
            assert_eq!(
                ctx.focused_view_id(fixture.window_id),
                Some(input_id),
                "the composer remains focused until the delayed terminal wakeup"
            );

            assert!(view.handle_terminal_wakeup(ctx));
        });

        app.read(|ctx| {
            assert_eq!(
                ctx.focused_view_id(fixture.window_id),
                Some(view.id()),
                "the PTY-owning session must receive input after the wakeup"
            );
        });
    });
}

#[test]
fn background_focus_reconciliation_does_not_steal_foreground_focus() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (foreground, _) = add_focus_test_session(&mut app, &fixture, true);
        let (background, _) = add_focus_test_session(&mut app, &fixture, false);
        let foreground_input_id = foreground.read(&app, |view, _| view.input_view.id());

        background.update(&mut app, |view, ctx| {
            view.update_process_input_focus(ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                ctx.focused_view_id(fixture.window_id),
                Some(foreground_input_id),
                "background ownership transitions must not change framework focus"
            );
        });
    });
}

/// A block model whose output is supplied directly, for exercising a block
/// that is materialized *after* its action already blocked. The production
/// path builds its model with `AIBlockModelImpl::new`, which needs a persisted
/// exchange the test harness does not have.
struct LateMaterializationBlockModel {
    conversation_id: AIConversationId,
    status: AIBlockOutputStatus,
}

impl AIBlockModel for LateMaterializationBlockModel {
    type View = TuiAIBlock;

    fn status(&self, _app: &AppContext) -> AIBlockOutputStatus {
        self.status.clone()
    }

    fn server_output_id(&self, _app: &AppContext) -> Option<ServerOutputId> {
        None
    }

    fn model_id(&self, _app: &AppContext) -> Option<LLMId> {
        None
    }

    fn base_model<'a>(&'a self, _app: &'a AppContext) -> Option<&'a LLMId> {
        None
    }

    fn inputs_to_render<'a>(&'a self, _app: &'a AppContext) -> &'a [AIAgentInput] {
        &[]
    }

    fn conversation_id(&self, _app: &AppContext) -> Option<AIConversationId> {
        Some(self.conversation_id)
    }

    fn on_updated_output(
        &self,
        _callback: OutputStatusUpdateCallback<Self::View>,
        _ctx: &mut ViewContext<Self::View>,
    ) {
    }

    fn request_type(&self, _app: &AppContext) -> AIRequestType {
        AIRequestType::Active
    }
}

/// Blocks `action` on user confirmation *before* any view renders it, then
/// appends the agent block that renders it — the restored/replayed-exchange
/// ordering, where the action event that created the blocker fired before the
/// block view existed.
fn materialize_already_blocked_action(
    app: &mut App,
    session: &ViewHandle<TuiTerminalSessionView>,
    action: AIAgentAction,
) -> ViewHandle<TuiAIBlock> {
    let (conversation_id, action_model, transcript) = session.update(app, |session, ctx| {
        let conversation_id = session
            .conversation_selection
            .update(ctx, |selection, ctx| {
                selection
                    .try_start_new_conversation(AgentViewEntryOrigin::Tui, ctx)
                    .expect("test conversation should start")
            });
        ctx.focus(&session.input_view);
        (
            conversation_id,
            session.ai_action_model.clone(),
            session.transcript.clone(),
        )
    });
    action_model.update(app, |model, ctx| {
        queue_tui_permission_action(model, action.clone(), conversation_id, ctx);
    });
    let status = AIBlockOutputStatus::Complete {
        output: Shared::new(AIAgentOutput {
            messages: vec![AIAgentOutputMessage {
                id: MessageId::new("late-action-message".to_owned()),
                message: AIAgentOutputMessageType::Action(action),
                citations: Vec::new(),
            }],
            ..Default::default()
        }),
    };
    transcript.update(app, |transcript, ctx| {
        transcript.append_agent_block_for_test(
            conversation_id,
            AIAgentExchangeId::new(),
            Rc::new(LateMaterializationBlockModel {
                conversation_id,
                status,
            }),
            ctx,
        )
    })
}

#[test]
fn foreground_session_focuses_an_already_blocked_question_when_materialized() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (session, _) = add_focus_test_session(&mut app, &fixture, true);
        let action = AIAgentAction {
            id: AIAgentActionId::from("late-question".to_owned()),
            task_id: TaskId::new("task".to_owned()),
            action: AIAgentActionType::AskUserQuestion {
                questions: vec![AskUserQuestionItem {
                    question_id: "question".to_owned(),
                    question: "Which option?".to_owned(),
                    question_type: AskUserQuestionType::MultipleChoice {
                        is_multiselect: false,
                        options: vec![AskUserQuestionOption {
                            label: "Alpha".to_owned(),
                            recommended: false,
                        }],
                        supports_other: false,
                    },
                }],
            },
            requires_result: true,
        };

        let block = materialize_already_blocked_action(&mut app, &session, action);
        let question = block.read(&app, |block, ctx| {
            let Some(TuiBlockingChild::AskQuestion(view)) = block.active_blocking_child(ctx) else {
                panic!("materialized question should be the active blocker");
            };
            view
        });

        app.read(|ctx| {
            assert!(ctx.check_view_or_child_focused(fixture.window_id, &question.id()));
        });
    });
}

#[test]
fn foreground_session_focuses_an_already_blocked_permission_when_materialized() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (session, _) = add_focus_test_session(&mut app, &fixture, true);
        // A generic tool call: upstream uses `InitProject`, which this fork's
        // action enum does not carry. `OpenCodeReview` takes the same
        // `sync_action_views` branch (neither question, file edits, documents,
        // nor shell command) and so materializes the same generic view with a
        // permission prompt.
        let action = AIAgentAction {
            id: AIAgentActionId::from("late-permission".to_owned()),
            task_id: TaskId::new("task".to_owned()),
            action: AIAgentActionType::OpenCodeReview,
            requires_result: true,
        };

        let block = materialize_already_blocked_action(&mut app, &session, action);
        let permission = block.read(&app, |block, ctx| {
            let Some(TuiBlockingChild::Permission(view)) = block.active_blocking_child(ctx) else {
                panic!("materialized permission prompt should be the active blocker");
            };
            view
        });

        app.read(|ctx| {
            assert!(ctx.check_view_or_child_focused(fixture.window_id, &permission.id()));
        });
    });
}

// ── Vim mode tests ───────────────────────────────────────────────────────────

/// The `/vim-mode` slash command static definition must be correctly
/// populated and marked TUI-executable.
///
/// Unlike upstream Warp (which gates surfaces via a `supported_surfaces`
/// field on `StaticCommand`), this fork gates TUI-executable commands via
/// `StaticCommand::supports_tui()` (see `static_commands/mod.rs`), so that is
/// what this test validates instead.
#[test]
fn vim_mode_slash_command_is_registered_in_command_registry() {
    let cmd = &slash_commands::VIM_MODE;
    assert_eq!(cmd.name, "/vim-mode");
    assert!(
        cmd.supports_tui(),
        "/vim-mode must be marked supports_tui so the TUI slash-command menu surfaces it"
    );
}

/// Executing the `/vim-mode` slash command must toggle and persist the
/// `AppEditorSettings::vim_mode` setting on each invocation.
#[test]
fn vim_mode_slash_command_persists_toggle() {
    App::test((), |mut app| async move {
        use warp::settings::AppEditorSettings;
        use warpui::SingletonEntity as _;

        let fixture = focus_test_fixture(&mut app);
        // AppEditorSettings is already registered by add_focus_test_session's
        // underlying register_all_settings; no explicit registration needed here.
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        assert!(
            !app.read(|ctx| AppEditorSettings::as_ref(ctx).vim_mode_enabled()),
            "vim mode should start disabled"
        );

        // First toggle: off -> on.
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::VIM_MODE, None, ctx);
        });
        assert!(
            app.read(|ctx| AppEditorSettings::as_ref(ctx).vim_mode_enabled()),
            "/vim-mode should enable vim mode on the first toggle"
        );
        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, _)| text.to_owned())
            }),
            Some(super::VIM_MODE_ENABLED_HINT.to_owned()),
            "should surface an enabled hint after enabling vim mode"
        );

        // Second toggle: on -> off.
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::VIM_MODE, None, ctx);
        });
        assert!(
            !app.read(|ctx| AppEditorSettings::as_ref(ctx).vim_mode_enabled()),
            "/vim-mode should disable vim mode on the second toggle"
        );
        assert_eq!(
            view.read(&app, |view, _| {
                view.transient_hint
                    .current()
                    .map(|(text, _)| text.to_owned())
            }),
            Some(super::VIM_MODE_DISABLED_HINT.to_owned()),
            "should surface a disabled hint after disabling vim mode"
        );
    });
}

/// The Vim mode indicator (INS/NOR/VIS/V-L/REP) must appear in the footer only
/// while Vim mode is enabled.
///
/// This test validates the accessor and the full render path.
#[test]
fn vim_mode_indicator_shown_only_when_vim_mode_is_enabled() {
    App::test((), |mut app| async move {
        use warp::settings::AppEditorSettings;
        use warpui::SingletonEntity as _;

        let fixture = focus_test_fixture(&mut app);
        // AppEditorSettings is already registered by add_focus_test_session's
        // underlying register_all_settings; no explicit registration needed here.
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        // Vim mode off: vim_mode_indicator returns None regardless of mode.
        app.read(|ctx| {
            let indicator = view.as_ref(ctx).vim_mode_indicator(ctx);
            assert!(
                indicator.is_none(),
                "indicator must be None when vim mode is disabled, got {indicator:?}"
            );
        });

        // Enable vim mode. The FSA starts in Insert mode, so the indicator
        // shows "INS", matching the GUI Vim status indicator.
        app.update(|ctx| {
            AppEditorSettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .vim_mode
                    .set_value(true, ctx)
                    .expect("failed to enable vim mode");
            });
        });
        app.read(|ctx| {
            let indicator = view.as_ref(ctx).vim_mode_indicator(ctx);
            assert_eq!(
                indicator,
                Some("INS"),
                "indicator must be INS in Insert mode when vim mode is enabled, got {indicator:?}"
            );
        });

        // Drive the input to Normal mode (Escape from Insert): indicator -> Some("NOR").
        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.handle_action(&crate::input::view::TuiInputAction::HandleEscape, ctx);
            });
        });
        app.read(|ctx| {
            let indicator = view.as_ref(ctx).vim_mode_indicator(ctx);
            assert_eq!(
                indicator,
                Some("NOR"),
                "indicator must be NOR in Normal mode when vim mode is enabled"
            );
        });
        // Verify via the full render path: the footer must contain NOR.
        let rendered = render_session(&mut app, &view, 80, 24).join("\n");
        assert!(
            rendered.contains("NOR"),
            "rendered footer must contain 'NOR' after Insert->Normal transition, got:\n{rendered}"
        );

        // Uppercase R enters continuous Replace mode and the footer reflects it.
        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.handle_action(
                    &crate::input::view::TuiInputAction::Editor(
                        crate::editor_element::TuiEditorAction::InsertChar('R'),
                    ),
                    ctx,
                );
            });
        });
        app.read(|ctx| {
            assert_eq!(view.as_ref(ctx).vim_mode_indicator(ctx), Some("REP"));
        });
        let rendered = render_session(&mut app, &view, 80, 24).join("\n");
        assert!(
            rendered.contains("REP"),
            "rendered footer must contain 'REP' in continuous Replace mode, got:\n{rendered}"
        );

        // Disable vim mode: indicator -> None again.
        app.update(|ctx| {
            AppEditorSettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .vim_mode
                    .set_value(false, ctx)
                    .expect("failed to disable vim mode");
            });
        });
        app.read(|ctx| {
            let indicator = view.as_ref(ctx).vim_mode_indicator(ctx);
            assert!(
                indicator.is_none(),
                "indicator must be None after vim mode is disabled, got {indicator:?}"
            );
        });
    });
}

/// `/cost` on a freshly-started (empty) conversation reports the same
/// unavailability hint as the GUI and hides nothing from the transcript.
#[test]
fn cost_slash_command_rejects_an_empty_conversation_like_the_gui() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection
                    .try_start_new_conversation(AgentViewEntryOrigin::Tui, ctx)
                    .expect("test conversation should start");
            });
        });

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::COST, None, ctx);
        });
        view.read(&app, |view, _| {
            assert!(view.hidden_response_summary_exchange_ids.is_empty());
            assert_eq!(
                view.transient_hint.current().map(|(text, _)| text),
                Some(COST_EMPTY_CONVERSATION_HINT),
            );
        });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Accepted prompt-and-command history (issue #387)
// ─────────────────────────────────────────────────────────────────────────────

/// Accepting a command row from the up-arrow history menu runs it through the
/// same shell-submission path as a typed command.
#[test]
fn accepted_command_history_executes_through_the_shell_submission_path() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let executed = Rc::new(RefCell::new(Vec::new()));
        app.update(|ctx| {
            let executed = executed.clone();
            ctx.subscribe_to_view(&view, move |_, event, _| {
                if let TuiTerminalSessionEvent::ExecuteCommand(event) = event {
                    executed.borrow_mut().push(event.command.clone());
                }
            });
        });

        view.update(&mut app, |view, ctx| {
            view.handle_accepted_prompt_and_command_history(
                "echo from history".to_owned(),
                TuiUpArrowHistoryItemKind::Command {
                    linked_workflow_data: None,
                },
                ctx,
            );
        });

        assert_eq!(executed.borrow().as_slice(), &["echo from history"]);
        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "");
    });
}

/// A command row's linked workflow data survives to the emitted
/// `ExecuteCommandEvent`, so a recalled workflow command still resolves back
/// to its workflow (e.g. for cost/telemetry attribution).
#[test]
fn accepted_command_history_preserves_workflow_metadata() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let executed = Rc::new(RefCell::new(Vec::new()));
        app.update(|ctx| {
            let executed = executed.clone();
            ctx.subscribe_to_view(&view, move |_, event, _| {
                if let TuiTerminalSessionEvent::ExecuteCommand(event) = event {
                    executed.borrow_mut().push((**event).clone());
                }
            });
        });

        view.update(&mut app, |view, ctx| {
            view.handle_accepted_prompt_and_command_history(
                "deploy production".to_owned(),
                TuiUpArrowHistoryItemKind::Command {
                    linked_workflow_data: Some(warp::tui_export::LinkedWorkflowData::Command(
                        "deploy {{environment}}".to_owned(),
                    )),
                },
                ctx,
            );
        });

        let executed = executed.borrow();
        let event = executed.as_slice().first().expect("command was executed");
        assert_eq!(event.command, "deploy production");
        assert_eq!(event.workflow_id, None);
        assert_eq!(
            event.workflow_command.as_deref(),
            Some("deploy {{environment}}")
        );
    });
}

/// Accepting a prompt row sends it to the session's selected AI conversation,
/// the same as a typed agent-mode submission.
#[test]
fn accepted_prompt_history_submits_to_the_selected_ai_conversation() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        // NOTE(adapted): the pin assumes a freshly activated session already
        // has a selected conversation. This fork's `send_prompt` requires one
        // explicitly (see `cost_slash_command_rejects_an_empty_conversation_like_the_gui`
        // above for the same pattern), so start one first.
        view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection
                    .try_start_new_conversation(AgentViewEntryOrigin::Tui, ctx)
                    .expect("test conversation should start");
            });
        });

        view.update(&mut app, |view, ctx| {
            view.handle_accepted_prompt_and_command_history(
                "explain the build".to_owned(),
                TuiUpArrowHistoryItemKind::Prompt,
                ctx,
            );
        });

        view.read(&app, |view, ctx| {
            let queries = view
                .conversation_selection
                .as_ref(ctx)
                .selected_conversation(ctx)
                .expect("selected conversation")
                .latest_exchange()
                .expect("accepted prompt should append an exchange")
                .input
                .iter()
                .filter_map(|input| input.user_query())
                .collect::<Vec<_>>();
            assert_eq!(queries, vec!["explain the build"]);
        });
        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "");
    });
}

#[test]
fn status_conversation_id_uses_the_selected_id_or_none() {
    let conversation_id = AIConversationId::new();
    assert_eq!(
        super::format_status_conversation_id(Some(conversation_id)),
        conversation_id.to_string()
    );
    assert_eq!(super::format_status_conversation_id(None), "None");
}

#[test]
fn agent_controlled_alt_screen_keeps_output_and_composer_visible() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, _| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.simulate_long_running_block("vim", "");
            let conversation_id = AIConversationId::new();
            let task_id = TaskId::new("alt-screen-terminal-use".to_owned());
            let block = terminal_model.block_list_mut().active_block_mut();
            block.set_agent_interaction_mode_for_requested_command(
                AIAgentActionId::from("alt-screen-command".to_owned()),
                Some(task_id.clone()),
                conversation_id,
            );
            block
                .set_agent_interaction_mode_for_agent_monitored_command(&task_id, conversation_id)
                .expect("command should become agent monitored");
            terminal_model.set_mode(Mode::SwapScreen {
                save_cursor_and_clear_screen: true,
            });
            for character in "ALT SCREEN".chars() {
                terminal_model.alt_screen_mut().input(character);
            }
        });

        assert!(view.read(&app, |view, _| {
            view.input_target().agent_editor_owns_input()
        }));
        let lines = render_session(&mut app, &view, 80, 12);
        let compact_output = lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        assert!(
            compact_output.contains("ALTSCREEN"),
            "alternate-screen output should remain visible:\n{}",
            lines.join("\n")
        );
        let alt_screen_row = lines
            .iter()
            .position(|line| line.contains("ALT"))
            .expect("alternate-screen output should start in the output area");
        let input_row = lines
            .iter()
            .position(|line| line.contains('▏'))
            .expect("agent-controlled alternate screen should render the composer");
        assert!(
            alt_screen_row < input_row,
            "alternate-screen output should render above the composer:\n{}",
            lines.join("\n")
        );
        // The footer's model segment proves the *normal* agent footer rendered, not
        // some alt-screen-specific stand-in. The pin hard-codes its cloud default
        // ("auto (cost-efficient)") here; BYOP has no built-in model list, so the
        // active model is whatever the user configured -- nothing, in a test app,
        // which yields the grayed-out placeholder from `placeholder_llm_info`. Read
        // the name the same way `footer_label_position`'s caller does instead of
        // naming a model this fork cannot select.
        let model_name = view.read(&app, |view, ctx| {
            LLMPreferences::as_ref(ctx)
                .get_active_base_model(ctx, Some(view.terminal_surface_id))
                .display_name
                .clone()
        });
        assert!(
            lines.iter().any(|line| line.contains(&model_name)),
            "the normal agent footer should remain visible (model {model_name:?}):\n{}",
            lines.join("\n")
        );
    });
}

/// Ported from the pin's `user_controlled_alt_screen_keeps_full_session_input_on_the_pty`
/// (`02b53fcd8`). The user-driven counterpart to
/// `agent_controlled_alt_screen_keeps_output_and_composer_visible` above: a
/// full-screen app the user is driving hands the whole pane to the alternate
/// screen (no composer, no normal footer), but still advertises manual agent
/// attachment via the same ghosted hint row the non-alt-screen LRC path uses.
#[test]
fn user_controlled_alt_screen_keeps_full_session_input_on_the_pty() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, _| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.simulate_long_running_block("vim", "");
            terminal_model.set_mode(Mode::SwapScreen {
                save_cursor_and_clear_screen: true,
            });
            for character in "USER ALT SCREEN".chars() {
                terminal_model.alt_screen_mut().input(character);
            }
        });

        assert!(view.read(&app, |view, _| view.input_target().pty_owns_input()));
        let lines = render_session(&mut app, &view, 80, 12);
        let compact_output = lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        assert!(
            compact_output.contains("USERALTSCREEN"),
            "alternate-screen output should render:\n{}",
            lines.join("\n")
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.chars().any(|glyph| "┌┐└┘─│▁▏▕▔".contains(glyph))),
            "user-controlled alternate screen should not render the agent composer:\n{}",
            lines.join("\n")
        );
        // Second, independent signal, mirroring the pin. The pin's negative
        // clause names its cloud default ("auto (cost-efficient)") because that
        // is what its composer footer always shows; BYOP has no built-in model
        // list, so the label here is whatever the user configured -- the
        // grayed-out `placeholder_llm_info` in a test app. Read the name the way
        // `agent_controlled_alt_screen_keeps_output_and_composer_visible` does
        // and assert its ABSENCE, so this catches a composer that renders with a
        // border vocabulary the glyph set above does not yet know about.
        let model_name = view.read(&app, |view, ctx| {
            LLMPreferences::as_ref(ctx)
                .get_active_base_model(ctx, Some(view.terminal_surface_id))
                .display_name
                .clone()
        });
        assert!(
            !lines.iter().any(|line| line.contains(&model_name)),
            "user-controlled alternate screen should not render the composer's \
             model label ({model_name}):\n{}",
            lines.join("\n")
        );
        let hint = view.read(&app, |view, ctx| {
            view.running_command_hint(ctx)
                .expect("alternate screen should have a running-command hint")
        });
        assert!(
            lines.iter().any(|line| line.trim() == hint),
            "user-controlled alternate screen should render the attach hint:\n{}",
            lines.join("\n")
        );
    });
}

#[test]
fn running_command_attachment_bindings_are_context_scoped() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        app.read(|ctx| {
            let attach = ctx
                .editable_bindings()
                .find(|binding| binding.name == ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME)
                .expect("running-command attach binding");
            assert_eq!(
                *attach.trigger,
                Trigger::Keystrokes(vec![Keystroke::parse("ctrl-shift-enter").unwrap()])
            );
            let detach = ctx
                .editable_bindings()
                .find(|binding| binding.name == DETACH_AGENT_FROM_RUNNING_COMMAND_BINDING_NAME)
                .expect("running-command detach binding");
            assert_eq!(
                *detach.trigger,
                Trigger::Keystrokes(vec![Keystroke::parse("escape").unwrap()])
            );

            let mut input_context = Context::default();
            input_context.set.insert("TuiInputView");
            assert!(!attach.in_context(&input_context));
            assert!(!detach.in_context(&input_context));

            input_context
                .set
                .insert(SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG);
            assert!(attach.in_context(&input_context));
            assert!(!detach.in_context(&input_context));

            input_context
                .set
                .remove(SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG);
            input_context
                .set
                .insert(SESSION_CAN_DETACH_AGENT_FROM_RUNNING_COMMAND_FLAG);
            assert!(!attach.in_context(&input_context));
            assert!(detach.in_context(&input_context));
        });
    });
}

/// Exercises the wired attach/detach mechanism behind
/// `ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME` end-to-end -- the binding
/// context test above only proves the keybinding is scoped correctly, not
/// that anything happens when it fires. Covers both ways the pin lets a user
/// give input back to the command: the dedicated escape-bound detach action,
/// and ctrl-c through `handle_terminal_use_interrupt`'s detach-first priority
/// (`02b53fcd8`, #390) -- without that priority, ctrl-c would instead arm the
/// TUI's own exit confirmation.
#[test]
fn manual_attach_and_detach_switch_running_command_input_ownership() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let interrupt_count = Rc::new(RefCell::new(0));
        let interrupt_count_for_events = interrupt_count.clone();
        app.update(|ctx| {
            ctx.subscribe_to_view(&view, move |_, event, _| {
                if matches!(event, TuiTerminalSessionEvent::InterruptPty) {
                    *interrupt_count_for_events.borrow_mut() += 1;
                }
            });
        });

        view.update(&mut app, |view, ctx| {
            view.terminal_model
                .lock()
                .simulate_long_running_block("cat", "");

            assert!(
                view.keymap_context(ctx)
                    .set
                    .contains(SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG),
                "an eligible user-controlled long-running command should advertise manual attach"
            );

            view.handle_action(&TuiTerminalSessionAction::AttachAgentToRunningCommand, ctx);

            assert!(
                view.terminal_model
                    .lock()
                    .block_list()
                    .active_block()
                    .is_agent_tagged_in(),
                "attaching should tag the agent into the active block"
            );
            assert!(
                view.agent_terminal_control_lock,
                "attaching should record that this composer installed the AI lock"
            );
            assert!(
                view.keymap_context(ctx)
                    .set
                    .contains(SESSION_CAN_DETACH_AGENT_FROM_RUNNING_COMMAND_FLAG)
            );
        });

        let lines = render_session(&mut app, &view, 80, 40);
        assert!(
            lines
                .iter()
                .any(|line| line.trim() == RUNNING_COMMAND_DETACH_HINT),
            "tagging in should replace the footer with the detach hint:\n{}",
            lines.join("\n")
        );

        // ctrl-c detaches instead of arming the exit confirmation while tagged in.
        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("unsent agent prompt", ctx);
            });
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);

            assert!(
                !view.exit_confirmation.is_armed(),
                "detaching a tagged-in command via ctrl-c must not also arm TUI exit"
            );
            assert!(
                !view
                    .terminal_model
                    .lock()
                    .block_list()
                    .active_block()
                    .is_agent_tagged_in(),
                "ctrl-c should have detached the agent"
            );
            assert!(
                !view.agent_terminal_control_lock,
                "detaching should clear the agent-control lock bookkeeping"
            );
            assert!(
                !view.try_detach_agent_from_running_command(ctx),
                "detaching an already-detached command should report no transition"
            );
        });
        assert_eq!(
            app.read(|ctx| input_text(&view, ctx)),
            "",
            "detaching must discard an unsent agent prompt"
        );
        assert_eq!(
            *interrupt_count.borrow(),
            0,
            "leaving the tagged composer via ctrl-c must not send an interrupt to the running command"
        );

        let lines = render_session(&mut app, &view, 80, 40);
        assert!(
            lines
                .iter()
                .all(|line| !line.contains(RUNNING_COMMAND_DETACH_HINT)),
            "detaching should remove the detach footer hint:\n{}",
            lines.join("\n")
        );
    });
}

/// A manually attached agent whose command simply finishes -- never detached
/// via escape or ctrl-c -- must still have its AI lock released, or the
/// composer stays hard-locked to AI mode forever with no way back to
/// autodetection. Ported from the pin's `running_command_completion_clears_transient_attachment_lock`
/// (`02b53fcd8`) for #390, adapted to assert this fork's local
/// `agent_terminal_control_lock` flag instead of the pin's
/// `InputTypeAutoDetectionSource::AgentTerminalControl` (see that field's doc
/// comment for why: threading a comparable source through this fork's shared
/// `BlocklistAIInputModel::set_input_config` is out of scope here).
#[test]
fn running_command_completion_clears_agent_terminal_control_lock() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        let block_id = view.update(&mut app, |view, ctx| {
            view.terminal_model
                .lock()
                .simulate_long_running_block("sleep 1", "");
            view.handle_action(&TuiTerminalSessionAction::AttachAgentToRunningCommand, ctx);
            assert!(
                view.agent_terminal_control_lock,
                "attaching should install the agent-control lock"
            );

            let mut terminal_model = view.terminal_model.lock();
            let block_id = terminal_model.block_list().active_block().id().clone();
            terminal_model.finish_block();
            block_id
        });

        view.update(&mut app, |view, ctx| {
            view.handle_block_completed(&block_id, ctx);
            assert!(
                !view.agent_terminal_control_lock,
                "the completing block should clear the agent-control lock this composer installed"
            );
        });
    });
}

/// Ported from the pin's `nld_reset_only_unlocks_after_agent_control_and_not_on_user_edit`
/// (`02b53fcd8`), adapted to this fork's `agent_terminal_control_lock` bool
/// and `TuiTerminalSessionView::{lock_for_agent_control, reset_after_agent_control}`
/// instead of the pin's `InputTypeAutoDetectionSource::AgentTerminalControl` --
/// see that field's doc comment for why threading a second autodetection-source
/// variant through the shared `BlocklistAIInputModel` is separately tracked
/// (#399/#254 item d) and out of scope here. The behavior under test --an
/// explicit Agent lock survives a user edit, but a lock installed for agent
/// terminal control resets once that control ends-- is unaffected by that
/// architectural difference.
#[test]
fn nld_reset_only_unlocks_after_agent_control_and_not_on_user_edit() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .ai_autodetection_enabled_internal
                    .set_value(true, ctx)
                    .expect("test setting should update");
            });
            view.input_view.update(ctx, |input, ctx| {
                input.exit_shell_mode(ctx);
                input.set_text("git status", ctx);
            });
            assert_eq!(
                view.ai_input_model.as_ref(ctx).input_config(),
                AI_LOCKED_CONFIG,
                "an explicit Agent lock should be retained while the user edits"
            );

            // User edits must not reinterpret an explicit Agent lock as stale
            // agent-control state.
            view.handle_input_content_changed(true, ctx);
            assert_eq!(
                view.ai_input_model.as_ref(ctx).input_config(),
                AI_LOCKED_CONFIG,
                "user edits must not unlock an explicit Agent lock"
            );

            // A lock installed for agent terminal control is reset when that
            // control completes, which restores the first post-agent prompt to
            // the setting-derived NLD state.
            view.lock_for_agent_control(ctx);
            view.reset_after_agent_control(ctx);
            assert_eq!(
                view.ai_input_model.as_ref(ctx).input_config(),
                AI_UNLOCKED_CONFIG,
                "agent-control completion should resume NLD"
            );
        });
    });
}

#[test]
fn footer_renders_shell_mode_sections_without_model_or_usage() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let row = render_status_footer_row(
                FooterSegments {
                    ordered: vec![
                        FooterSegment::ShellMode,
                        FooterSegment::WorkingDirectory("/home/user/warp".to_owned()),
                        FooterSegment::GitBranch("main".to_owned()),
                        FooterSegment::GitDiff {
                            files_changed: 2,
                            additions: 3,
                            deletions: 1,
                        },
                    ],
                },
                &builder,
            )
            .finish();
            let buffer = render_element(row, ctx, 120);
            assert_eq!(
                buffer[(0, 0)].fg,
                builder
                    .shell_command_accent_style()
                    .fg
                    .expect("shell command accent has a foreground")
            );
            let lines = buffer.to_lines();
            let line = lines.join("\n");

            assert_eq!(
                lines,
                vec![format!("{SHELL_MODE_HINT} /home/user/warp ↬ main | ☰ 2 • +3 -1")],
                "shell footer leads with the shell-mode indicator and hides model/usage"
            );
            assert!(
                line.starts_with(SHELL_MODE_HINT),
                "shell mode is the first segment"
            );
            assert!(
                !line.contains("TestModel"),
                "model segment is hidden in shell mode"
            );
            assert!(
                !line.contains("2.5 credits"),
                "usage segment is hidden in shell mode"
            );
        });
    });
}

#[test]
fn orchestration_tab_icon_replaces_identity_only_while_active_or_blocked() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let identity = AgentIdentity {
                glyph: "✠",
                style: TuiStyle::default().fg(Color::Blue),
            };
            for (status, expected_glyph) in [
                (ConversationStatus::InProgress, "●"),
                (ConversationStatus::TransientError, "●"),
                (ConversationStatus::WaitingForEvents, "●"),
                (
                    ConversationStatus::Blocked {
                        blocked_action: "approval".to_owned(),
                    },
                    "■",
                ),
            ] {
                assert_eq!(
                    orchestration_tab_icon(&status, &identity, &builder).0,
                    expected_glyph,
                );
            }
            for status in [
                ConversationStatus::Success,
                ConversationStatus::Error,
                ConversationStatus::Cancelled,
            ] {
                assert_eq!(
                    orchestration_tab_icon(&status, &identity, &builder),
                    (identity.glyph, identity.style),
                );
            }
        });
    });
}

#[test]
fn orchestration_tab_footer_advertises_down_without_shift_or_escape_hint() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let buffer = render_element(render_orchestration_tab_footer(&builder), ctx, 80);
            let footer = buffer.to_lines().join("\n");
            assert!(
                footer.contains("↓ to send a message"),
                "footer should advertise ↓: {footer}"
            );
            assert!(
                !footer.contains("Shift + ↓"),
                "footer must not advertise Shift + ↓: {footer}"
            );
            assert!(
                !footer.to_lowercase().contains("esc"),
                "footer must not advertise an Escape hint: {footer}"
            );
        });
    });
}

#[test]
fn alternate_screen_clears_orchestration_tab_focus_and_bindings() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.orchestration_tabs_focused = true;
            view.terminal_model.lock().process_bytes("\u{1b}[?1049h");
            view.focus_current_owner(ctx);
        });
        view.read(&app, |view, ctx| {
            assert!(!view.orchestration_tabs_focused);
            assert!(
                !view
                    .keymap_context(ctx)
                    .set
                    .contains(ORCHESTRATION_TAB_BAR_FOCUSED_FLAG)
            );
        });
    });
}

#[test]
fn orchestration_updates_refresh_only_the_focused_session() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (foreground, foreground_id) = add_focus_test_session(&mut app, &fixture, true);
        let (background, background_id) = add_focus_test_session(&mut app, &fixture, false);

        background.update(&mut app, |view, _| {
            view.orchestration_tabs_focused = true;
        });
        app.update(|ctx| {
            TuiOrchestrationModel::handle(ctx).update(ctx, |_, ctx| {
                ctx.notify();
            });
        });

        assert_eq!(
            app.read_model(&fixture.sessions, |sessions, _| {
                sessions.focused_session_id()
            }),
            Some(foreground_id)
        );
        assert!(
            app.read(|ctx| {
                ctx.check_view_or_child_focused(fixture.window_id, &foreground.id())
            })
        );
        assert!(background.read(&app, |view, _| view.orchestration_tabs_focused));

        app.update_model(&fixture.sessions, |sessions, ctx| {
            assert!(sessions.focus_session(background_id, ctx));
        });
        assert!(!background.read(&app, |view, _| view.orchestration_tabs_focused));
    });
}

fn tab_focused_context() -> Context {
    let mut context = Context::default();
    context.set.insert(super::TuiTerminalSessionView::ui_name());
    context.set.insert(ORCHESTRATION_TAB_BAR_FOCUSED_FLAG);
    context
}

fn input_only_context() -> Context {
    let mut context = Context::default();
    context.set.insert(crate::input::TuiInputView::ui_name());
    context
}

#[test]
fn focus_input_bindings_match_down_and_shift_down_in_tab_context_only() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        app.read(|ctx| {
            let down = Trigger::Keystrokes(vec![Keystroke::parse("down").unwrap()]);
            let shift_down = Trigger::Keystrokes(vec![Keystroke::parse("shift-down").unwrap()]);

            let focus_input_bindings: Vec<_> = ctx
                .editable_bindings()
                .filter(|b| b.name == "tui:orchestration_tabs:focus_input")
                .collect();
            assert_eq!(
                focus_input_bindings.len(),
                2,
                "down + shift-down bindings should be registered"
            );
            assert!(
                focus_input_bindings.iter().any(|b| *b.trigger == down),
                "plain down should focus the input"
            );
            assert!(
                focus_input_bindings
                    .iter()
                    .any(|b| *b.trigger == shift_down),
                "shift-down should remain an alias"
            );

            let tab_context = tab_focused_context();
            for binding in &focus_input_bindings {
                assert!(
                    binding.in_context(&tab_context),
                    "focus-input binding {:?} should match the tab-focused context",
                    binding.trigger
                );
            }

            let input_context = input_only_context();
            for binding in &focus_input_bindings {
                assert!(
                    !binding.in_context(&input_context),
                    "focus-input binding {:?} must not match a normal input context",
                    binding.trigger
                );
            }
        });
    });
}

#[test]
fn escape_binding_targets_main_agent_in_tab_context_only() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        app.read(|ctx| {
            let escape = Trigger::Keystrokes(vec![Keystroke::parse("escape").unwrap()]);
            let binding = ctx
                .editable_bindings()
                .find(|b| b.name == "tui:orchestration_tabs:focus_main")
                .expect("escape focus-main binding is registered");
            assert_eq!(*binding.trigger, escape);

            assert!(binding.in_context(&tab_focused_context()));
            assert!(!binding.in_context(&input_only_context()));
        });
    });
}

#[test]
fn orchestration_tab_navigation_bindings_remain_scoped_to_tab_context() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        app.read(|ctx| {
            let tab_context = tab_focused_context();
            let input_context = input_only_context();
            for (name, key) in [
                ("tui:orchestration_tabs:previous", "left"),
                ("tui:orchestration_tabs:next", "right"),
                ("tui:orchestration_tabs:tree_previous", "shift-tab"),
                ("tui:orchestration_tabs:tree_next", "tab"),
                ("tui:orchestration_tabs:first_child", "shift-left"),
                ("tui:orchestration_tabs:last_child", "shift-right"),
            ] {
                let trigger = Trigger::Keystrokes(vec![Keystroke::parse(key).unwrap()]);
                let binding = ctx
                    .editable_bindings()
                    .find(|b| b.name == name && *b.trigger == trigger)
                    .unwrap_or_else(|| panic!("missing {name} on {key}"));
                assert!(
                    binding.in_context(&tab_context),
                    "{name} {key} should match the tab-focused context"
                );
                assert!(
                    !binding.in_context(&input_context),
                    "{name} {key} must not match a normal input context"
                );
            }
        });
    });
}

/// Registers a session with a live active conversation, returning its view,
/// session id, and conversation id.
fn add_orchestration_session(
    app: &mut App,
    fixture: &FocusTestFixture,
    focus: bool,
) -> (
    ViewHandle<super::TuiTerminalSessionView>,
    TuiSessionId,
    AIConversationId,
) {
    let (view, manager) = add_test_terminal_session(app, fixture.window_id);
    let session_id = app.update(|ctx| {
        TuiSessions::register_session(&fixture.sessions, view.clone(), manager, focus, ctx)
    });
    let conversation_id = app.update(|ctx| {
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            // Zap's `start_new_conversation` has no `is_viewing_shared_session`
            // distinction folded into a 4th flag the way the pin's does; it
            // takes only `is_autoexecute_override` and `is_viewing_shared_session`.
            let conversation_id =
                history.start_new_conversation(session_id.surface_id(), false, false, ctx);
            history.set_active_conversation_id(conversation_id, session_id.surface_id(), ctx);
            conversation_id
        })
    });
    (view, session_id, conversation_id)
}

/// Registers a child session under a parent conversation.
fn add_orchestration_child(
    app: &mut App,
    fixture: &FocusTestFixture,
    parent_conversation_id: AIConversationId,
    name: &str,
) -> (
    ViewHandle<super::TuiTerminalSessionView>,
    TuiSessionId,
    AIConversationId,
) {
    let (view, manager) = add_test_terminal_session(app, fixture.window_id);
    let session_id = app.update(|ctx| {
        TuiSessions::register_session(&fixture.sessions, view.clone(), manager, false, ctx)
    });
    let conversation_id = app.update(|ctx| {
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            let conversation_id = history.start_new_child_conversation(
                session_id.surface_id(),
                name.to_owned(),
                parent_conversation_id,
                Some(Harness::Oz),
                ctx,
            );
            history.set_active_conversation_id(conversation_id, session_id.surface_id(), ctx);
            conversation_id
        })
    });
    (view, session_id, conversation_id)
}

/// `/orchestrate` requires an active conversation, mirroring the GUI's guard in
/// `execute_slash_command` (`app/src/terminal/input/slash_commands/mod.rs`). Regression
/// coverage for the TUI dispatch arm added to close #325's TUI gap: `/orchestrate`
/// deliberately keeps `kind() == SlashCommandKind::Other` (see `commands::ORCHESTRATE`'s doc
/// comment), so without a name guard specifically for it, this would silently fall into
/// `execute_tui_slash_command`'s GUI-only `debug_assert!(false, ...)` catch-all instead of
/// this hint.
#[test]
fn orchestrate_slash_command_requires_active_conversation() {
    App::test((), |mut app| async move {
        assert!(slash_commands::ORCHESTRATE.supports_tui());

        let fixture = focus_test_fixture(&mut app);
        app.update(TuiPaneGroup::register);
        let (view, session_id) = add_focus_test_session(&mut app, &fixture, true);

        // A TUI session is never conversation-less at rest: `TuiConversationSelection::new`
        // eagerly starts one and `select_new_conversation` immediately replaces it, so
        // `selected_conversation_id` is `Some` for the whole normal lifetime of a session.
        // The single window in which it is `None` is the one `defer_replacement_conversation`
        // opens -- the selected conversation is removed and the replacement is only created on
        // a later tick via `ctx.spawn`. That is the state this guard exists for, so reproduce
        // it the same way `conversation_selection_tests.rs` does and run `/orchestrate` inside
        // it, before the deferred replacement lands.
        let conversation_id = view.read(&app, |view, ctx| {
            view.conversation_selection
                .as_ref(ctx)
                .selected_conversation_id(ctx)
                .expect("a TUI session starts with a conversation selected")
        });
        app.update(|ctx| {
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |_, ctx| {
                ctx.emit(BlocklistAIHistoryEvent::RemoveConversation {
                    terminal_view_id: session_id.surface_id(),
                    conversation_id,
                });
            });
        });
        view.read(&app, |view, ctx| {
            assert_eq!(
                view.conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx),
                None,
                "removing the selected conversation should open the replacement window"
            );
        });

        let task = "write tests".to_owned();
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::ORCHESTRATE, Some(&task), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(
                view.transient_hint.current(),
                Some((
                    ORCHESTRATE_REQUIRES_CONVERSATION_HINT,
                    crate::transient_hint::TransientHintTone::Muted
                ))
            );
        });
    });
}

/// `/orchestrate` requires a non-blank task after the command, mirroring the GUI's
/// `argument.map(|a| a.trim()).filter(|a| !a.is_empty())` guard. Covers both a missing
/// argument (menu-selected with no text typed) and a whitespace-only one.
#[test]
fn orchestrate_slash_command_requires_task_argument() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        app.update(TuiPaneGroup::register);
        let (view, _session_id, _conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::ORCHESTRATE, None, ctx);
        });
        view.read(&app, |view, _| {
            assert_eq!(
                view.transient_hint.current(),
                Some((
                    ORCHESTRATE_REQUIRES_TASK_HINT,
                    crate::transient_hint::TransientHintTone::Muted
                ))
            );
        });

        let blank = "   ".to_owned();
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::ORCHESTRATE, Some(&blank), ctx);
        });
        view.read(&app, |view, _| {
            assert_eq!(
                view.transient_hint.current(),
                Some((
                    ORCHESTRATE_REQUIRES_TASK_HINT,
                    crate::transient_hint::TransientHintTone::Muted
                ))
            );
        });
    });
}

/// `/orchestrate` with a real conversation and task must route into
/// `TuiPaneGroup::spawn_local_child_agents` rather than falling into
/// `execute_tui_slash_command`'s GUI-only `debug_assert!(false, ...)` catch-all -- that
/// assertion is live in test builds, so the regression this guards against would panic this
/// test, not merely misbehave silently. Combined with the guard-path coverage above, this
/// proves the name-guarded `SlashCommandKind::Other` arm is reached for exactly the right
/// conditions.
///
/// The rest of the pipeline is deliberately NOT driven to completion here, matching this
/// project's established convention for the same real-PTY/real-CLI gap:
/// `local_harness_launch_tests.rs` leaves `prepare_local_harness_child_launch` (the async half
/// that shells out to validate the `claude` CLI) untested for the same reason, and
/// `pane_group_tests.rs`'s own doc comment calls `spawn_local_child_agents`'s real-session-
/// creation half "untested, per project convention for real-PTY paths". What IS covered, by
/// `pane_group_tests.rs::finish_spawning_local_child_agent_registers_and_tracks_child`, is that
/// once a child session materializes, it reaches `TuiOrchestrationModel::snapshot` -- i.e. the
/// orchestration tab bar. This test closes the remaining gap: that `/orchestrate` in the TUI
/// actually reaches `TuiPaneGroup::spawn_local_child_agents` in the first place.
#[test]
fn orchestrate_slash_command_routes_to_tui_pane_group_without_falling_into_gui_only_catch_all() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        app.update(TuiPaneGroup::register);
        let (view, _session_id, _conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("/orchestrate write tests", ctx);
            });
        });

        let task = "write tests".to_owned();
        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::ORCHESTRATE, Some(&task), ctx);
        });

        // Neither guard hint fired: a real conversation and a real task were supplied, so
        // the arm proceeded past both early returns into TuiPaneGroup.
        view.read(&app, |view, _| {
            let hint = view.transient_hint.current().map(|(text, _)| text);
            assert_ne!(hint, Some(ORCHESTRATE_REQUIRES_CONVERSATION_HINT));
            assert_ne!(hint, Some(ORCHESTRATE_REQUIRES_TASK_HINT));
        });
        // The dispatch arm clears the composer before handing off to TuiPaneGroup, same as
        // every other executing slash command.
        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "");
    });
}

#[test]
fn escape_from_child_tab_switches_to_root_and_clears_tab_focus() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (parent_view, parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // Focus the child session and point its conversation selection at the child
        // conversation so the orchestration snapshot resolves the parent as root.
        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
            view.orchestration_tabs_focused = true;
            view.refresh_orchestration_tab_bar(ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                child_view
                    .as_ref(ctx)
                    .orchestration_tab_bar
                    .as_ref(ctx)
                    .tree_root_key(),
                Some(parent_conversation_id.to_string()),
                "tab bar should expose the parent as the tree root"
            );
        });

        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::FocusMainOrchestrationTab, ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                TuiSessions::as_ref(ctx).focused_session_id(),
                Some(parent_session_id),
                "escape should switch focus to the root/main session"
            );
            assert!(
                !child_view.as_ref(ctx).orchestration_tabs_focused,
                "child tab focus should be cleared"
            );
            assert!(
                !parent_view.as_ref(ctx).orchestration_tabs_focused,
                "parent tab focus should remain cleared"
            );
            assert!(
                ctx.check_view_or_child_focused(fixture.window_id, &parent_view.id()),
                "root session input should own focus after escape"
            );
        });
    });
}

#[test]
fn escape_with_root_selected_clears_tab_focus_without_switching() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (parent_view, parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (_child_view, _child_session_id, _child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // Point the parent session's conversation selection at the root conversation so
        // the orchestration snapshot resolves the root as both root and selected.
        parent_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    parent_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
            view.orchestration_tabs_focused = true;
            view.refresh_orchestration_tab_bar(ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                parent_view
                    .as_ref(ctx)
                    .orchestration_tab_bar
                    .as_ref(ctx)
                    .tree_root_key(),
                Some(parent_conversation_id.to_string()),
                "root tab bar should expose the root as the tree root"
            );
        });

        parent_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::FocusMainOrchestrationTab, ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                TuiSessions::as_ref(ctx).focused_session_id(),
                Some(parent_session_id),
                "escape with root selected should not switch sessions"
            );
            assert!(
                !parent_view.as_ref(ctx).orchestration_tabs_focused,
                "root tab focus should be cleared"
            );
            assert!(
                ctx.check_view_or_child_focused(fixture.window_id, &parent_view.id()),
                "root session input should own focus after escape"
            );
        });
    });
}

// ---------------------------------------------------------------------------
// Child-agent Ctrl+C kill (upstream fd16dceb3, APP-5031).
//
// Ported whole: every case below is local-only. Upstream's two cloud-run
// cases (`TuiCloudRunView` arming its own kill window, and
// `kill_child_agent_removes_session_and_conversation_from_map` which drives
// `initialize_remote_child_session`) are dropped -- `cloud_run_view.rs` and
// remote children do not exist in this fork (`DECLINED.md` #290). The
// session-removal assertion those covered is kept here instead, against the
// local `/orchestrate` child session that does exist.
// ---------------------------------------------------------------------------

#[test]
fn kill_child_hint_constant_matches_expected_text() {
    assert_eq!(CTRL_C_KILL_CHILD_HINT, "ctrl-c again to kill child agent");
}

#[test]
fn orchestration_child_selected_footer_shows_kill_hint() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let buffer = render_element(
                render_orchestration_child_selected_tab_footer(&builder, 0),
                ctx,
                120,
            );
            let footer = buffer.to_lines().join("\n");
            assert!(
                footer.contains("Ctrl+C"),
                "child-selected footer should show Ctrl+C: {footer}"
            );
            assert!(
                footer.contains("kill sub-agent"),
                "child-selected footer should describe the kill action: {footer}"
            );
            assert!(
                !footer.contains("nested"),
                "leaf footer must not name a nested blast radius: {footer}"
            );
            let group_buffer = render_element(
                render_orchestration_child_selected_tab_footer(&builder, 2),
                ctx,
                120,
            );
            let group_footer = group_buffer.to_lines().join("\n");
            assert!(
                group_footer.contains("kill sub-agent +2 nested"),
                "group footer should name the blast radius: {group_footer}"
            );
            assert!(
                footer.contains('\u{2193}'),
                "child-selected footer should still show the send-message \u{2193} hint: {footer}"
            );
        });
    });
}

#[test]
fn ctrl_c_on_child_tab_with_tabs_focused_kills_immediately() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (_parent_view, parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // Focus the child session and point its selection at the child conversation so
        // the orchestration snapshot resolves the parent as root and the child as selected.
        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
            view.orchestration_tabs_focused = true;
            view.refresh_orchestration_tab_bar(ctx);
        });

        // Verify the snapshot sees the child tab as the selected non-root tab.
        app.read(|ctx| {
            assert!(
                child_view
                    .as_ref(ctx)
                    .is_child_conversation_selected(ctx)
                    .is_some(),
                "child conversation should be detected as selected"
            );
        });

        // Single ctrl-c should kill the child immediately (no double-press window).
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        app.read(|ctx| {
            assert!(
                !child_view.as_ref(ctx).exit_confirmation.is_armed(),
                "kill path must not arm the exit confirmation window"
            );
            assert_eq!(
                child_view.as_ref(ctx).child_kill_armed_conversation,
                None,
                "kill path must not set child_kill_armed_conversation"
            );
            // The child conversation should be deleted from history.
            assert!(
                BlocklistAIHistoryModel::as_ref(ctx)
                    .conversation(&child_conversation_id)
                    .is_none(),
                "child conversation should be deleted from history after kill"
            );
            // Focus should return to the parent session.
            assert_eq!(
                TuiSessions::as_ref(ctx).focused_session_id(),
                Some(parent_session_id),
                "focus should return to the root/main agent after kill"
            );
        });
    });
}

/// Covers the half of upstream's `kill_child_agent_removes_session_and_conversation_from_map`
/// that survives de-clouding: the retained TUI session backing the killed child is
/// dropped from [`TuiSessions`], not just its conversation from history. Upstream
/// asserts this through `initialize_remote_child_session`; here the equivalent
/// session is the hidden local one a `/orchestrate` child runs in.
#[test]
fn kill_child_agent_removes_the_retained_child_session() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (_parent_view, _parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        let sessions_before = app.read(|ctx| TuiSessions::as_ref(ctx).len());

        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
            view.orchestration_tabs_focused = true;
            view.refresh_orchestration_tab_bar(ctx);
        });
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                TuiSessions::as_ref(ctx).len(),
                sessions_before - 1,
                "the killed child's retained session should be dropped from the registry"
            );
            assert!(
                TuiSessions::as_ref(ctx).session(child_session_id).is_none(),
                "the killed child's session id should no longer resolve"
            );
        });
    });
}

#[test]
fn ctrl_c_on_child_conversation_without_tab_focus_arms_kill_window() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (_parent_view, _parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // Focus the child session but without tab-bar focus.
        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
            // orchestration_tabs_focused stays false (default)
        });

        // First ctrl-c should arm the kill window, not delete the conversation.
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        app.read(|ctx| {
            assert!(
                child_view.as_ref(ctx).exit_confirmation.is_armed(),
                "first ctrl-c on a child conversation should arm the kill window"
            );
            assert_eq!(
                child_view.as_ref(ctx).child_kill_armed_conversation,
                Some(child_conversation_id),
                "child_kill_armed_conversation should target the viewed child"
            );
            // Conversation should NOT be deleted yet.
            assert!(
                BlocklistAIHistoryModel::as_ref(ctx)
                    .conversation(&child_conversation_id)
                    .is_some(),
                "child conversation must not be deleted after only one ctrl-c"
            );
        });
    });
}

#[test]
fn footer_shows_kill_hint_when_child_kill_window_is_armed() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (_parent_view, _parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // Focus the child session (no tab-bar focus).
        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
        });

        // Footer before any ctrl-c: should NOT show the kill hint.
        let lines_before = render_footer_lines(&mut app, &child_view, 80);
        assert!(
            !lines_before.join("\n").contains(CTRL_C_KILL_CHILD_HINT),
            "footer should not show kill hint before arming: {lines_before:?}"
        );

        // First ctrl-c: arm the kill window.
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        // Footer after first ctrl-c: must show the child-kill hint, not the exit hint.
        let lines_after = render_footer_lines(&mut app, &child_view, 80);
        assert_eq!(
            lines_after,
            vec![CTRL_C_KILL_CHILD_HINT],
            "footer must show the kill-child hint when the kill window is armed"
        );
        let lines_str = lines_after.join("\n");
        assert!(
            !lines_str.contains(CTRL_C_EXIT_HINT),
            "kill-armed footer must not show the exit hint: {lines_str:?}"
        );
    });
}

#[test]
fn second_ctrl_c_within_window_kills_the_child_agent() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (_parent_view, parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // Focus the child session (no tab-bar focus).
        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
        });

        // First ctrl-c: arms the kill window.
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        // Confirm the kill window is armed.
        assert!(
            child_view.read(&app, |view, _| view.exit_confirmation.is_armed()),
            "kill window must be armed after first ctrl-c"
        );

        // Second ctrl-c within the window: kills the child.
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                child_view.as_ref(ctx).child_kill_armed_conversation,
                None,
                "kill window should be cleared after the kill"
            );
            assert!(
                !child_view.as_ref(ctx).exit_confirmation.is_armed(),
                "exit window should be cleared after the kill"
            );
            // Child conversation should be gone from history.
            assert!(
                BlocklistAIHistoryModel::as_ref(ctx)
                    .conversation(&child_conversation_id)
                    .is_none(),
                "child conversation should be deleted after double ctrl-c kill"
            );
            // Focus should return to the root/main session.
            assert_eq!(
                TuiSessions::as_ref(ctx).focused_session_id(),
                Some(parent_session_id),
                "focus should return to the root/main agent after kill"
            );
        });
    });
}

#[test]
fn ctrl_c_on_root_conversation_does_not_trigger_kill_path() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (parent_view, _parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (_child_view, _child_session_id, _child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // The parent session is focused and pointing at the root conversation.
        // ctrl-c should follow the normal exit path, not the kill path.
        parent_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        app.read(|ctx| {
            assert!(
                parent_view.as_ref(ctx).exit_confirmation.is_armed(),
                "ctrl-c on root should arm the normal exit window"
            );
            assert_eq!(
                parent_view.as_ref(ctx).child_kill_armed_conversation,
                None,
                "ctrl-c on root must not set child_kill_armed_conversation"
            );
        });
    });
}

#[test]
fn lapsed_kill_window_does_not_kill_child_on_next_ctrl_c() {
    // If the 1-second kill window lapses without a second press, the next
    // ctrl-c is a fresh first press and must arm a new window, NOT kill the child.
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (_parent_view, _parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // Focus the child session (no tab-bar focus).
        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
        });

        // First ctrl-c: arms the kill window.
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });
        assert!(
            child_view.read(&app, |view, _| view.exit_confirmation.is_armed()),
            "kill window must be armed after first ctrl-c"
        );

        // Simulate window lapse: disarm + clear the armed conversation (as the
        // timer callback does), without triggering the kill.
        child_view.update(&mut app, |view, _| {
            view.exit_confirmation.disarm();
            view.child_kill_armed_conversation = None;
        });

        // Next ctrl-c: armed = None, should arm a new kill window rather than kill.
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        app.read(|ctx| {
            // Child conversation must still exist -- lapse prevented the kill.
            assert!(
                BlocklistAIHistoryModel::as_ref(ctx)
                    .conversation(&child_conversation_id)
                    .is_some(),
                "child conversation must survive a ctrl-c after the window lapsed"
            );
            // A new kill window should now be armed, not executed.
            assert!(
                child_view.as_ref(ctx).exit_confirmation.is_armed(),
                "a new kill window should be armed by the post-lapse ctrl-c"
            );
            assert_eq!(
                child_view.as_ref(ctx).child_kill_armed_conversation,
                Some(child_conversation_id),
                "post-lapse ctrl-c should re-arm for the same child"
            );
        });
    });
}

#[test]
fn killing_child_does_not_exit_tui_parent_session_remains_alive() {
    // Killing a child agent must never cause the whole TUI to exit.
    // The parent session must remain focused and its conversation intact.
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (parent_view, parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");

        // Kill via the tab-bar-focused single-press path.
        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
            view.orchestration_tabs_focused = true;
            view.refresh_orchestration_tab_bar(ctx);
        });
        // Single ctrl-c kills the child.
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        // TUI must still be alive: the singleton models exist, the parent
        // session is focused, and the parent's conversation is untouched.
        app.read(|ctx| {
            assert!(
                ctx.has_singleton_model::<TuiSessions>(),
                "TuiSessions singleton must survive the child kill"
            );
            assert!(
                ctx.has_singleton_model::<TuiOrchestrationModel>(),
                "TuiOrchestrationModel singleton must survive the child kill"
            );
            assert_eq!(
                TuiSessions::as_ref(ctx).focused_session_id(),
                Some(parent_session_id),
                "parent session must be focused after child kill"
            );
            assert!(
                BlocklistAIHistoryModel::as_ref(ctx)
                    .conversation(&parent_conversation_id)
                    .is_some(),
                "parent conversation must survive child kill"
            );
            assert!(
                parent_view
                    .as_ref(ctx)
                    .child_kill_armed_conversation
                    .is_none(),
                "parent view must not have a stale kill window after kill"
            );
        });
    });
}

#[test]
fn new_slash_command_kills_descendant_agents() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (parent_view, _parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (_child_view, _child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");
        let (_grandchild_view, _grandchild_session_id, grandchild_conversation_id) =
            add_orchestration_child(&mut app, &fixture, child_conversation_id, "grandchild");

        parent_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    parent_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.execute_tui_slash_command(&slash_commands::NEW, None, ctx);
        });

        app.read(|ctx| {
            let history = BlocklistAIHistoryModel::as_ref(ctx);
            assert!(
                history.conversation(&child_conversation_id).is_none(),
                "/new should delete direct child conversations"
            );
            assert!(
                history.conversation(&grandchild_conversation_id).is_none(),
                "/new should delete nested child conversations"
            );
            let new_conversation_id = parent_view
                .as_ref(ctx)
                .conversation_selection
                .as_ref(ctx)
                .selected_conversation_id(ctx)
                .expect("/new should select a replacement conversation");
            assert_ne!(new_conversation_id, parent_conversation_id);
            assert!(history.conversation(&new_conversation_id).is_some());
        });
    });
}

/// Multi-level subtree kill (upstream 683d40782): with the bar focused,
/// Ctrl+C on the drilled-in anchor kills that agent and its whole loaded
/// subtree, and can never fall through to conversation-cancel + app-exit.
#[test]
fn ctrl_c_on_drilled_anchor_with_tabs_focused_kills_the_subtree() {
    let _flag = warp_core::features::FeatureFlag::MultiLevelOrchestration.override_enabled(true);
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (_parent_view, parent_session_id, parent_conversation_id) =
            add_orchestration_session(&mut app, &fixture, true);
        let (child_view, child_session_id, child_conversation_id) =
            add_orchestration_child(&mut app, &fixture, parent_conversation_id, "child");
        let (_grandchild_view, _grandchild_session_id, grandchild_conversation_id) =
            add_orchestration_child(&mut app, &fixture, child_conversation_id, "grandchild");

        // Select the group child: it re-anchors the bar, occupying the
        // main-tab slot (selected == anchor != root).
        app.update(|ctx| {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(child_session_id, ctx);
            });
        });
        child_view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.select_existing_conversation(
                    child_conversation_id,
                    AgentViewEntryOrigin::Tui,
                    ctx,
                );
            });
            view.refresh_orchestration_tab_state(ctx);
            view.orchestration_tabs_focused = true;
            view.refresh_orchestration_tab_bar(ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                child_view.as_ref(ctx).bar_focused_kill_target(ctx),
                Some((child_conversation_id, 1)),
                "the drilled-in anchor must be the kill target with its blast radius"
            );
        });

        // A single ctrl-c kills the anchor and its subtree — it must not
        // cancel the conversation and arm the app-exit window.
        child_view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
        });

        app.read(|ctx| {
            assert!(
                !child_view.as_ref(ctx).exit_confirmation.is_armed(),
                "the drilled-anchor kill must not arm the exit confirmation window"
            );
            let history = BlocklistAIHistoryModel::as_ref(ctx);
            assert!(
                history.conversation(&child_conversation_id).is_none(),
                "the anchor conversation must be deleted"
            );
            assert!(
                history.conversation(&grandchild_conversation_id).is_none(),
                "the anchor's subtree must be deleted with it"
            );
            assert!(
                history.conversation(&parent_conversation_id).is_some(),
                "the root must survive"
            );
            assert_eq!(
                TuiSessions::as_ref(ctx).focused_session_id(),
                Some(parent_session_id),
                "focus should return to the root agent after the subtree kill"
            );
        });
    });
}

// MCP install-flow variable masking (#602).

#[test]
fn mcp_menu_footer_replaces_status_with_controls() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let footer = render_mcp_menu_footer(
                &TuiUiBuilder::from_app(ctx),
                Some(TuiMcpAction::Stop(TuiMcpServerId::FileBased(1))),
                true,
            )
            .finish();
            assert_eq!(
                render_element(footer, ctx, 120).to_lines(),
                vec![
                    "Enter to stop  Ctrl+R to log out & remove credentials  Esc to close"
                        .to_owned()
                ],
            );
        });
    });
}

#[test]
fn mcp_menu_footer_hides_unavailable_primary_control() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let logout_only = render_mcp_menu_footer(&builder, None, true).finish();
            assert_eq!(
                render_element(logout_only, ctx, 120).to_lines(),
                vec!["Ctrl+R to log out & remove credentials  Esc to close".to_owned()],
            );
            let close_only = render_mcp_menu_footer(&builder, None, false).finish();
            assert_eq!(
                render_element(close_only, ctx, 120).to_lines(),
                vec!["Esc to close".to_owned()],
            );
        });
    });
}

#[test]
fn mcp_menu_footer_hides_unavailable_logout_control() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let footer = render_mcp_menu_footer(
                &TuiUiBuilder::from_app(ctx),
                Some(TuiMcpAction::Start(TuiMcpServerId::FileBased(1))),
                false,
            )
            .finish();
            assert_eq!(
                render_element(footer, ctx, 120).to_lines(),
                vec!["Enter to start  Esc to close".to_owned()],
            );
        });
    });
}

#[test]
fn mcp_primary_action_hints_match_available_actions() {
    let id = TuiMcpServerId::FileBased(1);
    assert_eq!(
        mcp_primary_action_hint(TuiMcpAction::Start(id)),
        Some("to start")
    );
    assert_eq!(
        mcp_primary_action_hint(TuiMcpAction::Stop(id)),
        Some("to stop")
    );
    assert_eq!(
        mcp_primary_action_hint(TuiMcpAction::Retry(id)),
        Some("to retry")
    );
    assert_eq!(
        mcp_primary_action_hint(TuiMcpAction::ReopenAuthorization(id)),
        Some("to authenticate")
    );
    assert_eq!(
        mcp_primary_action_hint(TuiMcpAction::Enable(id)),
        Some("to install and enable")
    );
    assert_eq!(mcp_primary_action_hint(TuiMcpAction::LogOut(id)), None);
}


// --- `/copy-debugging-id` (upstream b4070d6a9) ---

/// The footer hint slot must carry an error-toned notice when the conversation has no
/// server token yet. `footer_hint()` reads `transient_hint.current()` verbatim when
/// present, so asserting on it covers what the footer renders.
#[test]
fn copy_debugging_id_shows_error_hint_when_no_server_token() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::COPY_DEBUGGING_ID, None, ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(
                view.transient_hint.current(),
                Some((
                    super::COPY_DEBUGGING_ID_NO_TOKEN_HINT,
                    crate::transient_hint::TransientHintTone::Error
                )),
                "/copy-debugging-id with no server token must set the no-token error hint"
            );
        });
    });
}

/// The same hint must actually reach the rendered session, not just the hint slot.
#[test]
fn copy_debugging_id_footer_hint_renders_in_session() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.update(&mut app, |view, ctx| {
            view.execute_tui_slash_command(&slash_commands::COPY_DEBUGGING_ID, None, ctx);
        });

        let rendered = render_session(&mut app, &view, 80, 24).join("\n");
        assert!(
            rendered.contains(super::COPY_DEBUGGING_ID_NO_TOKEN_HINT),
            "rendered session must contain the no-token hint in the footer; got:\n{rendered}"
        );
    });
}

/// Ported from the pin (`42effe840`): the Figma git-diff item leads with a
/// file count, so a change with no counted lines (binary, mode-only,
/// whitespace-normalised) is still visible.
#[test]
fn git_diff_status_matches_figma_file_count_content_and_styles() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let row = render_status_footer_row(
                FooterSegments {
                    ordered: vec![FooterSegment::GitDiff {
                        files_changed: 6,
                        additions: 31,
                        deletions: 12,
                    }],
                },
                &builder,
            )
            .finish();
            let buffer = render_element(row, ctx, 80);
            assert_eq!(buffer.to_lines(), vec!["☰ 6 • +31 -12".to_owned()]);
            assert_eq!(
                buffer[(0, 0)].fg,
                builder
                    .muted_text_style()
                    .fg
                    .expect("file glyph should use the muted foreground"),
            );
            assert_eq!(
                buffer[(
                    (0..buffer.area.width)
                        .find(|column| buffer[(*column, 0)].symbol() == "+")
                        .expect("addition glyph should render"),
                    0,
                )]
                    .fg,
                builder
                    .diff_added_style()
                    .fg
                    .expect("addition count should use the added foreground"),
            );
            assert_eq!(
                buffer[(
                    (0..buffer.area.width)
                        .find(|column| buffer[(*column, 0)].symbol() == "-")
                        .expect("deletion glyph should render"),
                    0,
                )]
                    .fg,
                builder
                    .diff_removed_style()
                    .fg
                    .expect("deletion count should use the removed foreground"),
            );

            let file_only = render_status_footer_row(
                FooterSegments {
                    ordered: vec![FooterSegment::GitDiff {
                        files_changed: 1,
                        additions: 0,
                        deletions: 0,
                    }],
                },
                &builder,
            )
            .finish();
            assert_eq!(
                render_element(file_only, ctx, 80).to_lines(),
                vec!["☰ 1".to_owned()],
                "binary or zero-line changes should remain visible through their file count",
            );
        });
    });
}

/// Ported from the pin (`42effe840`). `GitBranchStatus` reads the same local
/// `git status` metadata the plain branch item does -- no Warp backend is
/// involved -- and adds the upstream tracking counts.
#[test]
fn git_branch_status_matches_figma_content_styles_and_tracking_variants() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let builder = TuiUiBuilder::from_app(ctx);
            let status = render_element(
                render_git_branch_status(
                    "main",
                    false,
                    Some("1".to_owned()),
                    Some("2".to_owned()),
                    &builder,
                ),
                ctx,
                80,
            );
            assert_eq!(status.to_lines(), vec!["⊢ main • ↑1 ↓2".to_owned()]);
            let muted = builder
                .muted_text_style()
                .fg
                .expect("muted status text has a foreground");
            let accent = builder
                .accent_text_style()
                .fg
                .expect("branch status arrows have a foreground");
            assert_eq!(status[(0, 0)].fg, muted);
            assert_eq!(status[(9, 0)].fg, accent);
            assert_eq!(status[(10, 0)].fg, muted);
            assert_eq!(status[(12, 0)].fg, accent);
            assert_eq!(status[(13, 0)].fg, muted);

            assert_eq!(
                render_element(
                    render_git_branch_status("main", false, Some("1".to_owned()), None, &builder),
                    ctx,
                    80,
                )
                .to_lines(),
                vec!["⊢ main • ↑1".to_owned()]
            );
            assert_eq!(
                render_element(
                    render_git_branch_status("main", false, None, Some("2".to_owned()), &builder),
                    ctx,
                    80,
                )
                .to_lines(),
                vec!["⊢ main • ↓2".to_owned()]
            );
            assert_eq!(
                render_element(
                    render_git_branch_status("main", true, None, None, &builder),
                    ctx,
                    80,
                )
                .to_lines(),
                vec!["⊢ main • ⇅".to_owned()]
            );
            assert_eq!(
                render_element(
                    render_git_branch_status("main", false, None, None, &builder),
                    ctx,
                    80,
                )
                .to_lines(),
                vec!["⊢ main".to_owned()]
            );
        });
    });
}

/// Ported from the pin (`42effe840`): the composite branch item is a superset
/// of the plain one, so enabling both must not print the branch name twice.
#[test]
fn composite_git_branch_status_suppresses_the_plain_branch_item() {
    let branch_only = TuiStatuslineConfig {
        order: TuiStatuslineItem::ALL.to_vec(),
        enabled: vec![TuiStatuslineItem::GitBranch],
    };
    assert!(should_render_plain_git_branch(&branch_only));

    let branch_and_status = TuiStatuslineConfig {
        order: TuiStatuslineItem::ALL.to_vec(),
        enabled: vec![
            TuiStatuslineItem::GitBranch,
            TuiStatuslineItem::GitBranchStatus,
        ],
    };
    assert!(!should_render_plain_git_branch(&branch_and_status));
}

/// Ported from the pin (`42effe840`). The TUI creates a blank conversation
/// eagerly, and `/copy-debugging-id` must be offered against it the same way
/// the GUI offers it against an active conversation -- the command is about a
/// *local* debugging identifier, so nothing here depends on being signed in.
#[test]
fn copy_debugging_id_available_in_active_commands_at_zero_state() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        view.read(&app, |view, ctx| {
            let has_copy_debugging_id = view
                .slash_commands_source
                .as_ref(ctx)
                .active_commands()
                .any(|(_, command)| command.kind() == SlashCommandKind::CopyDebuggingId);
            assert!(
                has_copy_debugging_id,
                "/copy-debugging-id must be available for the blank active conversation",
            );
        });
    });
}

/// Ported from the pin (`42effe840`): exactly one model-picker row is badged
/// as the active execution profile's default. The badged row is *not*
/// necessarily the selected one -- a per-surface override or the BYOP
/// last-used model can move the effective active model off the profile
/// default, which is the whole reason the badge exists.
#[test]
fn model_menu_labels_the_profile_default_model() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::ToggleModelMenu, ctx);
        });

        view.read(&app, |view, ctx| {
            let default_name = LLMPreferences::as_ref(ctx)
                .get_active_profile_base_model(ctx, Some(view.terminal_surface_id))
                .display_name
                .clone();
            let snapshot = view
                .model_menu
                .as_ref(ctx)
                .snapshot(ctx)
                .expect("model menu should be open");
            let default_row = snapshot
                .rows
                .iter()
                .find(|row| row.title == default_name)
                .expect("profile default model should be listed");
            assert!(
                default_row
                    .state_suffix
                    .as_deref()
                    .is_some_and(|suffix| suffix.contains("(default)"))
            );
            assert_eq!(
                snapshot
                    .rows
                    .iter()
                    .filter(|row| {
                        row.state_suffix
                            .as_deref()
                            .is_some_and(|suffix| suffix.contains("(default)"))
                    })
                    .count(),
                1
            );
        });
    });
}

/// The footer element rendered to a buffer rather than to text, so a test can
/// assert per-cell styling. Mirrors the pin's `render_footer` helper, renamed
/// here to avoid reading like the view method of the same name that it calls.
fn render_footer_buffer(
    app: &mut App,
    view: &ViewHandle<super::TuiTerminalSessionView>,
    width: u16,
) -> TuiBuffer {
    app.update(|ctx| {
        let footer = view.as_ref(ctx).render_footer(ctx).finish();
        render_element(footer, ctx, width)
    })
}

/// Ported from the pin (`42effe840`). The footer's auto-approve entry is a
/// control, not a status label: it renders whenever the item is enabled and
/// the session is not in shell mode, and reports its state through colour --
/// muted while auto-approve is off, `success_glyph_style` while it is on. It
/// has to stay on the row while off, otherwise there would be nothing to click
/// to turn auto-approve on.
#[test]
fn enabled_auto_approve_indicator_is_always_visible_with_state_aware_color() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            // A fresh session's input defaults to `InputType::Shell`, and
            // `render_footer` hides the agent-only auto-approve entry in shell
            // mode. Switch to AI input mode the same way a real conversation
            // entry would.
            view.ai_input_model.update(ctx, |input_model, ctx| {
                input_model.set_input_type(InputType::AI, ctx);
            });
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection
                    .try_start_new_conversation(AgentViewEntryOrigin::Tui, ctx)
                    .expect("test conversation should start")
            });
        });
        set_enabled_statusline_items(&mut app, vec![TuiStatuslineItem::AutoApprove]);

        let disabled = render_footer_buffer(&mut app, &view, 80);
        assert_eq!(disabled.to_lines(), vec!["▶▶".to_owned()]);
        assert_eq!(
            disabled[(0, 0)].fg,
            app.read(|ctx| {
                TuiUiBuilder::from_app(ctx)
                    .muted_text_style()
                    .fg
                    .expect("muted text style should have a foreground")
            }),
            "the control is muted while auto-approve is off"
        );

        view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.toggle_pending_query_autoexecute(ctx);
            });
        });
        let enabled = render_footer_buffer(&mut app, &view, 80);
        assert_eq!(enabled.to_lines(), vec!["▶▶".to_owned()]);
        assert_eq!(
            enabled[(0, 0)].fg,
            app.read(|ctx| {
                TuiUiBuilder::from_app(ctx)
                    .success_glyph_style()
                    .fg
                    .expect("success glyph style should have a foreground")
            }),
            "the control turns success-coloured while auto-approve is on"
        );

        restore_default_statusline(&mut app);
    });
}

/// Ported from the pin (`42effe840`). The footer's auto-approve control and
/// the warping indicator's auto-approve control can be on screen at the same
/// time, so each keeps its own retained `MouseStateHandle`: pressing one must
/// neither arm nor cancel the other. Before the split both read a single
/// `auto_approve_mouse` field, so an armed click on either control was
/// observable -- and cancellable -- through the other.
#[test]
fn auto_approve_controls_retain_independent_mouse_state() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.read(&app, |view, _| {
            assert!(
                !Arc::ptr_eq(
                    &view.footer_auto_approve_mouse,
                    &view.warping_auto_approve_mouse,
                ),
                "footer and warping controls must not share retained mouse state",
            );
        });

        // Unlike the pin's, this fork's `render_warping_indicator` takes the
        // conversation whose auto-approve feedback window it styles, so a
        // conversation has to exist before the row can be rendered.
        let conversation_id = view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection
                    .try_start_new_conversation(AgentViewEntryOrigin::Tui, ctx)
                    .expect("test conversation should start")
            })
        });

        // Both controls in one retained tree, which is the situation the split
        // exists for: the warping row on top, the footer control beneath it.
        let (mut element, scene, buffer) = view.read(&app, |view, ctx| {
            let builder = TuiUiBuilder::from_app(ctx);
            let controls = warpui_core::elements::tui::TuiFlex::column()
                .child(view.render_warping_indicator(
                    "Phosphorizing",
                    Duration::ZERO,
                    conversation_id,
                    ctx,
                ))
                .child(view.render_auto_approve_statusline(&builder, ctx))
                .finish();
            render_retained_element(controls, ctx, 80, 2)
        });
        let lines = buffer.to_lines();
        let footer_row = lines
            .iter()
            .position(|line| line.trim_end() == "▶▶")
            .expect("footer control should render") as u16;
        let footer_col = first_visible_column(&lines[usize::from(footer_row)]) as u16;
        let (warping_col, warping_row) = footer_label_position(&buffer, "▶▶ Auto approve off");

        assert!(dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &left_mouse_down(footer_col, footer_row),
        ));
        view.read(&app, |view, _| {
            assert!(view.footer_auto_approve_mouse.lock().unwrap().is_clicked());
            assert!(!view.warping_auto_approve_mouse.lock().unwrap().is_clicked());
        });
        assert!(
            dispatch_session_event(
                &app,
                &view,
                &mut element,
                scene.clone(),
                &left_mouse_up(footer_col, footer_row),
            ),
            "warping control must not cancel the footer's armed click",
        );
        view.read(&app, |view, _| {
            assert!(!view.footer_auto_approve_mouse.lock().unwrap().is_clicked());
            assert!(!view.warping_auto_approve_mouse.lock().unwrap().is_clicked());
        });

        assert!(dispatch_session_event(
            &app,
            &view,
            &mut element,
            scene.clone(),
            &left_mouse_down(warping_col, warping_row),
        ));
        view.read(&app, |view, _| {
            assert!(!view.footer_auto_approve_mouse.lock().unwrap().is_clicked());
            assert!(view.warping_auto_approve_mouse.lock().unwrap().is_clicked());
        });
        assert!(
            dispatch_session_event(
                &app,
                &view,
                &mut element,
                scene,
                &left_mouse_up(warping_col, warping_row),
            ),
            "footer control must not cancel the warping control's armed click",
        );
    });
}

/// Ported from the pin (`42effe840`). The zero state is drawn above the input,
/// so anything that appears or disappears *between* them -- the shell-bootstrap
/// status line -- must not move it. The pin's own
/// `bootstrap_renders_starting_shell_above_input` above pins the ORDER (status
/// above input); this pins POSITION STABILITY, which order alone does not imply:
/// a bootstrap row inserted above the zero state would keep the order and still
/// shift the title down a row on every shell restart.
///
/// One-string adaptation: this fork's zero-state title is "Phosphor Agent"
/// (`zero_state.rs`), the pin's is "Warp Agent CLI".
#[test]
fn zero_state_position_stays_stable_across_shell_bootstrap() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);

        let ready_lines = render_session(&mut app, &view, 80, 40);
        assert!(
            ready_lines
                .iter()
                .all(|line| line.trim() != "Starting shell..."),
            "ready state must not render the bootstrap hint:\n{}",
            ready_lines.join("\n")
        );

        view.update(&mut app, |view, _| {
            view.terminal_model.lock().block_list_mut().reinit_shell();
        });
        let bootstrap_lines = render_session(&mut app, &view, 80, 40);
        assert!(
            bootstrap_lines
                .iter()
                .any(|line| line.trim() == "Starting shell..."),
            "bootstrap state must render the starting-shell hint:\n{}",
            bootstrap_lines.join("\n")
        );

        let title_row = |lines: &[String]| {
            lines
                .iter()
                .position(|line| line.contains("Phosphor Agent"))
                .unwrap_or_else(|| panic!("zero-state title should render:\n{}", lines.join("\n")))
        };
        assert_eq!(
            title_row(&bootstrap_lines),
            title_row(&ready_lines),
            "zero state must not shift when the bootstrap hint disappears\nbootstrap:\n{}\nready:\n{}",
            bootstrap_lines.join("\n"),
            ready_lines.join("\n")
        );
    });
}

/// Ported from the pin (`42effe840`). The manual-attach counterpart to
/// `agent_controlled_alt_screen_keeps_output_and_composer_visible` above: the
/// agent was tagged into a long-running command the *user* started, and that
/// command then switched to the alternate screen. The two mechanisms are
/// separately covered (`manual_attach_and_detach_switch_running_command_input_ownership`
/// for attach/detach, the agent-controlled test for alt-screen + composer);
/// only this combination was untested, and it is the one where the composer can
/// plausibly be lost -- a full-screen app owns the whole pane in the
/// user-controlled case.
///
/// Leaving the tagged composer with ctrl-c must detach, not interrupt: the
/// alternate-screen program keeps running and never sees the keystroke.
#[test]
fn tagged_in_alt_screen_keeps_output_and_composer_visible() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        let interrupt_count = Rc::new(RefCell::new(0));
        let interrupt_count_for_events = interrupt_count.clone();
        app.update(|ctx| {
            ctx.subscribe_to_view(&view, move |_, event, _| {
                if matches!(event, TuiTerminalSessionEvent::InterruptPty) {
                    *interrupt_count_for_events.borrow_mut() += 1;
                }
            });
        });
        view.update(&mut app, |view, ctx| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.simulate_long_running_block("vim", "");
            terminal_model.set_mode(Mode::SwapScreen {
                save_cursor_and_clear_screen: true,
            });
            for character in "TAGGED ALT SCREEN".chars() {
                terminal_model.alt_screen_mut().input(character);
            }
            drop(terminal_model);
            view.handle_action(&TuiTerminalSessionAction::AttachAgentToRunningCommand, ctx);
        });

        assert!(view.read(&app, |view, _| {
            view.input_target().agent_editor_owns_input()
        }));
        let lines = render_session(&mut app, &view, 80, 12);
        let compact_output = lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        assert!(
            compact_output.contains("TAGGEDALTSCREEN"),
            "tagged-in alternate-screen output should remain visible:\n{}",
            lines.join("\n")
        );
        assert!(
            lines.iter().any(|line| line.contains('▏')),
            "tagged-in alternate screen should render the composer:\n{}",
            lines.join("\n")
        );
        assert!(
            lines
                .iter()
                .any(|line| line.trim() == RUNNING_COMMAND_DETACH_HINT),
            "tagged-in alternate screen should show the detach footer hint:\n{}",
            lines.join("\n")
        );
        view.update(&mut app, |view, ctx| {
            view.handle_action(&TuiTerminalSessionAction::Interrupt, ctx);
            assert!(
                !view.exit_confirmation.is_armed(),
                "leaving a tagged alternate-screen LRC must not arm TUI exit"
            );
            assert!(view.input_target().pty_owns_input());
            assert!(
                !view
                    .terminal_model
                    .lock()
                    .block_list()
                    .active_block()
                    .is_agent_tagged_in()
            );
        });
        assert_eq!(
            *interrupt_count.borrow(),
            0,
            "leaving the tagged composer must not send ctrl-c to the alternate-screen command"
        );
    });
}

/// Ported from the pin (`42effe840`). Every surface stacked against the input
/// -- the statusline below it and the inline menus above it -- is aligned on the
/// input border's OUTER edge, not on the text inside the border. The one
/// deliberate exception is the read-only (shortcuts) sheet, whose *background*
/// starts at the outer edge while its text keeps a one-cell internal padding.
///
/// Two adaptations, neither of which changes what is asserted:
///
/// * The pin anchors the statusline row on its cloud default's display name,
///   "auto (cost-efficient)". This fork is BYOP-only, so the active model is
///   whatever the user configured; read the name from `LLMPreferences` the way
///   `agent_controlled_alt_screen_keeps_output_and_composer_visible` does. The
///   statusline's `Model` segment renders only outside shell mode, and a fresh
///   session defaults to `InputType::Shell`, so switch to AI input first --
///   exactly what `footer_model_label_is_a_bounded_click_target` does.
/// * The pin anchors the inline-menu row on the literal "/agent". Locate the
///   selected row by its selection background at the border column instead, so
///   the assertion does not depend on which command this fork's mixer ranks
///   first. That background lookup IS the pin's second assertion, so nothing is
///   dropped: a selection that starts one cell in fails here as a panic rather
///   than an `assert_eq`.
#[test]
fn input_adjacent_surfaces_follow_figma_outer_edge_alignment() {
    App::test((), |mut app| async move {
        app.update(crate::keybindings::init);
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, ctx| {
            view.ai_input_model.update(ctx, |input_model, ctx| {
                input_model.set_input_type(InputType::AI, ctx);
            });
        });
        let model_name = view.read(&app, |view, ctx| {
            LLMPreferences::as_ref(ctx)
                .get_active_base_model(ctx, Some(view.terminal_surface_id))
                .display_name
                .clone()
        });

        let lines = render_session(&mut app, &view, 80, 24);
        let input_border_column = lines
            .iter()
            .find(|line| line.contains('▏'))
            .map(|line| first_visible_column(line))
            .unwrap_or_else(|| panic!("input border must render:\n{}", lines.join("\n")));
        let statusline_column = lines
            .iter()
            .find(|line| line.contains(&model_name))
            .map(|line| first_visible_column(line))
            .unwrap_or_else(|| {
                panic!(
                    "statusline must render (model {model_name:?}):\n{}",
                    lines.join("\n")
                )
            });
        assert_eq!(
            statusline_column,
            input_border_column,
            "statusline must begin at the input border's outer edge:\n{}",
            lines.join("\n")
        );

        view.update(&mut app, |view, ctx| {
            view.input_view.update(ctx, |input, ctx| {
                input.set_text("/", ctx);
            });
        });
        futures_lite::future::yield_now().await;
        let buffer = render_session_buffer(&mut app, &view, 80, 24);
        let lines = buffer.to_lines();
        let selection_background =
            app.read(|ctx| TuiUiBuilder::from_app(ctx).slash_command_selection_background());
        let slash_command_column = lines
            .iter()
            .enumerate()
            .find(|(row, _)| {
                buffer[(input_border_column as u16, *row as u16)].bg == selection_background
            })
            .map(|(_, line)| first_visible_column(line))
            .unwrap_or_else(|| {
                panic!(
                    "selected inline-menu background must begin at the input border's outer edge:\n{}",
                    lines.join("\n")
                )
            });
        assert_eq!(
            slash_command_column,
            input_border_column,
            "inline-menu content must begin at the input border's outer edge:\n{}",
            lines.join("\n")
        );

        view.update(&mut app, |view, ctx| {
            view.suggestions_mode.update(ctx, |mode, ctx| {
                mode.set_mode(
                    TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Shortcuts),
                    ctx,
                );
            });
        });
        let buffer = render_session_buffer(&mut app, &view, 80, 24);
        let lines = buffer.to_lines();
        let (shortcuts_row, shortcuts_column) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.contains("Shortcuts"))
            .map(|(row, line)| (row, first_visible_column(line)))
            .unwrap_or_else(|| panic!("shortcuts surface must render:\n{}", lines.join("\n")));
        assert_eq!(
            shortcuts_column,
            input_border_column + 1,
            "shortcuts text keeps its designed one-cell internal padding:\n{}",
            lines.join("\n")
        );
        assert_eq!(
            buffer[(input_border_column as u16, shortcuts_row as u16)].bg,
            app.read(|ctx| TuiUiBuilder::from_app(ctx).read_only_menu_background()),
            "shortcuts background must begin at the input border's outer edge"
        );
    });
}
