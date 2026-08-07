use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use instant::Instant;
use warp::appearance::Appearance;
use warp::settings::{AISettings, TuiStatuslineConfig, TuiStatuslineItem};
use warp::terminal::model::ansi::{Handler, InputBufferValue};
use warp::tui_export::{
    AIAgentActionId, AIAgentExchangeId, AIConversationAutoexecuteMode, AIConversationId,
    AgentViewEntryOrigin, AgentViewState, BlockPadding, BlocklistAIHistoryModel,
    ConversationStatus, Harness, InputType, LLMPreferences, PtyIntent, PtyIntentEvent, SizeInfo,
    SizeUpdate, TaskId, export_conversation_markdown, register_tui_session_view_test_singletons,
    slash_commands,
};
use warp_core::settings::Setting as _;
use warp_editor::model::CoreEditorModel;
use warpui::platform::WindowStyle;
use warpui::{
    AddWindowOptions, EntityIdMap, ModelHandle, ReadModel, SingletonEntity, UpdateModel, ViewHandle,
};
use warpui_core::r#async::Timer;
use warpui_core::elements::tui::{
    Color, TuiBuffer, TuiBufferExt, TuiConstrainedBox, TuiConstraint, TuiContainer, TuiElement,
    TuiEvent, TuiEventContext, TuiLayoutContext, TuiPaintContext, TuiPaintSurface, TuiPoint,
    TuiRect, TuiScene, TuiScreenPosition, TuiSize, TuiStyle, TuiText,
};
use warpui_core::event::ModifiersState;
use warpui_core::keymap::{Context, Keystroke, Trigger};
use warpui_core::presenter::tui::TuiPresenter;
use warpui_core::{App, AppContext, TuiView, TypedActionView as _, WindowInvalidation};

use super::{
    AUTO_APPROVE_DISABLED_HINT, AUTO_APPROVE_ENABLED_HINT, AUTO_APPROVE_FEEDBACK_DURATION,
    AUTO_APPROVE_TOGGLE_BINDING_NAME, COST_CONVERSATION_IN_PROGRESS_HINT,
    COST_EMPTY_CONVERSATION_HINT,
    COST_NO_ACTIVE_CONVERSATION_HINT, CTRL_C_EXIT_HINT, ConversationRestoreState, FooterSegment,
    FooterSegments, INLINE_MENU_TOP_PADDING_ROWS, LOADING_CONVERSATION_HINT, LOG_BUNDLE_FAILED_HINT,
    SHELL_MODE_HINT, TuiConversationRestoreOrigin, TuiTerminalSessionAction,
    TuiQueuedFollowUp, TuiTerminalSessionEvent, TuiTerminalSessionView,
    cost_command_unavailable_hint, export_file_success_message, log_bundle_success_message,
    raw_prompt_if_not_blank, render_status_footer_row,
};
use crate::autoupdate::TuiAutoupdater;
use crate::inline_menu::MAX_INLINE_MENU_ROWS;
use crate::keybindings::{
    CONTEXTUAL_PLAN_TOGGLE_BINDING_NAME, KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG,
    PLAN_TOGGLE_AVAILABLE_FLAG, PLAN_TOGGLE_BINDING_NAME, TUI_BINDING_GROUP,
};
use crate::root_view::RootTuiView;
use crate::session_registry::{TuiSessionId, TuiSessions};
use crate::statusline_config_view::TuiStatuslineConfigEvent;
use crate::terminal_block::{block_content_rows, should_render_terminal_block};
use crate::terminal_use::TuiInputTarget;
use crate::test_fixtures::{add_test_semantic_selection, add_test_terminal_session};
use crate::transcript_view::TRANSCRIPT_BLOCK_SPACING;
use crate::tui_builder::TuiUiBuilder;
use crate::usage::render_context_usage_entry;

struct FocusTestFixture {
    window_id: warpui_core::WindowId,
    sessions: ModelHandle<TuiSessions>,
}

#[test]
fn footer_supports_arbitrary_order_and_branch_without_a_leading_arrow() {
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
                    ],
                },
                &builder,
            )
            .finish();
            assert_eq!(
                render_element(row, ctx, 120).to_lines(),
                vec!["43% context • feature/statusline • Auto-queue • /tmp/warp".to_owned()],
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
#[test]
fn enabled_auto_indicators_render_only_while_their_effective_states_are_on() {
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
            vec!["Auto-approve • Queued".to_owned()],
        );

        view.update(&mut app, |view, ctx| {
            view.conversation_selection.update(ctx, |selection, ctx| {
                selection.toggle_pending_query_autoexecute(ctx);
            });
            view.queued_follow_up = None;
        });
        assert!(render_footer_lines(&mut app, &view, 80).is_empty());

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

        let picker_id = view.read(&app, |view, ctx| {
            let picker = view
                .statusline_config_view
                .as_ref()
                .expect("statusline picker should be open");
            assert!(view.input_view.as_ref(ctx).is_empty(ctx));
            assert!(picker.as_ref(ctx).is_focused(ctx));
            picker.id()
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
        let mut element = ctx
            .render_tui_view(view.window_id(ctx), view.id())
            .expect("session view should render");
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

/// Locates the footer's active-model label in the rendered buffer, returning
/// the (column, row) of its first cell. Counts chars (not bytes) so multi-byte
/// glyphs earlier in the footer row don't shift the column.
fn model_label_position(buffer: &TuiBuffer, model_name: &str) -> (u16, u16) {
    let lines = buffer.to_lines();
    for (row, line) in lines.iter().enumerate() {
        if let Some(byte_offset) = line.find(model_name) {
            let col = line[..byte_offset].chars().count() as u16;
            return (col, row as u16);
        }
    }
    panic!(
        "model label {:?} not found in rendered footer:\n{}",
        model_name,
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
                super::TransientHintTone::Success
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
                super::TransientHintTone::Success
            ))
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
        let (label_col, label_row) = model_label_position(&buffer, &model_name);
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
                });
            }
            view.handle_typeahead_event(ctx);
        });
        assert_eq!(app.read(|ctx| input_text(&view, ctx)), "ec");

        view.update(&mut app, |view, ctx| {
            view.terminal_model.lock().input_buffer(InputBufferValue {
                buffer: "echo hi".to_owned(),
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
            .find(|(_, line)| line.contains('┌') || line.contains('─'))
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
                .any(|line| line.contains('┌') || line.contains('─')),
            "LRC must keep the input editor hidden:\n{}",
            lines.join("\n")
        );
        // The interrupt affordance renders as a ghosted row in the input's
        // slot while the command owns input.
        assert!(
            lines
                .iter()
                .any(|line| line.trim() == crate::input_hints::LONG_RUNNING_COMMAND_HINT),
            "LRC must render the interrupt hint row:\n{}",
            lines.join("\n")
        );
    });
}

/// Visible startup-script execution also routes input to the PTY, but it is
/// not a user-controlled command: the interrupt hint row must not appear.
#[test]
fn visible_startup_script_shows_no_interrupt_hint() {
    App::test((), |mut app| async move {
        let fixture = focus_test_fixture(&mut app);
        let (view, _) = add_focus_test_session(&mut app, &fixture, true);
        view.update(&mut app, |view, _| {
            let mut terminal_model = view.terminal_model.lock();
            terminal_model.block_list_mut().reinit_shell();
            terminal_model.update_blockheight_items(TRANSCRIPT_BLOCK_SPACING.block_padding, 0.0);
            // Advance past WarpInput, then leave an unfinished startup-script
            // block with visible output owning PTY input.
            terminal_model.simulate_block("bootstrap", "");
            terminal_model.simulate_long_running_block("shell init", "startup output\r\n");
        });
        assert!(
            view.read(&app, |view, _| view.input_target().pty_owns_input()),
            "fixture should route input to the PTY during the visible startup script"
        );

        let lines = render_session(&mut app, &view, 80, 40);
        assert!(
            !lines
                .iter()
                .any(|line| line.trim() == crate::input_hints::LONG_RUNNING_COMMAND_HINT),
            "startup-script execution must not advertise the interrupt hint:\n{}",
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
            presenter.present(ctx, &view, TuiRect::new(0, 0, 120, 40))
        });
        let lines = frame.buffer.to_lines();
        let title_row = lines
            .iter()
            .position(|line| line.contains("Warp Agent"))
            .expect("zero state should render the Warp Agent title");
        assert!(
            title_row < 28,
            "zero-state title should render in the transcript area:\n{}",
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
                vec!["TestModel • /home/user/warp ↬ main • 18% context • +3 -1"],
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
                vec![format!("{SHELL_MODE_HINT} /home/user/warp ↬ main • +3 -1")],
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
        assert!(
            row.starts_with(&format!("{model_name} ")),
            "the model-led status row renders in place of the callout: {row}"
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
