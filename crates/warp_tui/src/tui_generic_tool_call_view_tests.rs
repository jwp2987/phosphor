use serde_json::json;
use warp::tui_export::{
    AIAgentAction, AIAgentActionId, AIAgentActionResultType, AIAgentActionType, AIConversationId,
    SuggestNewConversationResult, TaskId, queue_tui_permission_action,
    register_tui_session_view_test_singletons,
};
use warp_core::execution_mode::{AppExecutionMode, ExecutionMode};
use warpui::platform::WindowStyle;
use warpui::{AddWindowOptions, App};
use warpui_core::elements::tui::{Color, TuiBufferExt, TuiRect};
use warpui_core::presenter::tui::TuiPresenter;
use warpui_core::{TuiView as _, WindowInvalidation};

use super::TuiGenericToolCallView;
use crate::test_fixtures::{TestHostView, add_test_action_model};
use crate::tui_builder::TuiUiBuilder;

#[test]
fn mcp_permission_details_are_structured_and_human_readable() {
    App::test((), |mut app| async move {
        // A CallMCPTool action blocks on confirmation through the real preprocess
        // pipeline, which reads permissions/settings singletons; provision them.
        register_tui_session_view_test_singletons(&mut app);
        let action_model = add_test_action_model(&mut app);
        let action = AIAgentAction {
            id: AIAgentActionId::from("mcp-action".to_owned()),
            task_id: TaskId::new("task".to_owned()),
            action: AIAgentActionType::CallMCPTool {
                server_id: None,
                name: "create_issue".to_owned(),
                input: json!({
                    "title": "Fix permission UI",
                    "priority": 1,
                }),
            },
            requires_result: true,
        };
        let action_for_queue = action.clone();
        let conversation_id = AIConversationId::new();
        let action_model_for_view = action_model.clone();
        let view = app.update(|ctx| {
            let (window_id, _) = ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| TestHostView,
            );
            ctx.add_tui_view(window_id, |ctx| {
                TuiGenericToolCallView::new(
                    action,
                    false,
                    action_model_for_view,
                    conversation_id,
                    ctx,
                )
            })
        });

        app.read(|ctx| {
            let view = view.as_ref(ctx);
            assert!(view.permission_prompt.is_none());
            // No server id on this action, so the question falls back to naming
            // just the tool (not the old "this MCP tool" phrasing).
            assert_eq!(
                view.permission_question(None),
                "Is it OK if I call MCP tool create_issue?"
            );
            // With a known server, both identities surface in the question.
            assert_eq!(
                view.permission_question(Some("github")),
                "Is it OK if I call MCP tool create_issue on github?"
            );
            let details = view.details(None);
            assert!(details.starts_with("create_issue\n{"));
            assert!(details.contains("\"priority\": 1"));
            assert!(details.contains("\"title\": \"Fix permission UI\""));
            // The details body labels the tool with its server when known.
            let details_with_server = view.details(Some("github"));
            assert!(details_with_server.starts_with("create_issue on github\n{"));
            assert!(details_with_server.contains("\"priority\": 1"));
        });

        action_model.update(&mut app, |action_model, ctx| {
            queue_tui_permission_action(action_model, action_for_queue, conversation_id, ctx);
        });
        // Pump the async preprocess so the action blocks and its prompt materializes.
        crate::test_fixtures::settle().await;
        app.read(|ctx| {
            let prompt = view
                .as_ref(ctx)
                .active_permission_prompt(ctx)
                .expect("blocked action should materialize its permission prompt");
            assert!(prompt.as_ref(ctx).is_active(ctx));
        });
        let mut presenter = TuiPresenter::new();
        let frame = app.update(|ctx| {
            let prompt = view
                .as_ref(ctx)
                .active_permission_prompt(ctx)
                .expect("blocked action should materialize its permission prompt");
            let mut invalidation = WindowInvalidation::default();
            invalidation.updated.insert(view.id());
            invalidation.updated.insert(prompt.id());
            invalidation
                .updated
                .extend(prompt.as_ref(ctx).child_view_ids(ctx));
            presenter.invalidate(&invalidation, ctx, view.window_id(ctx));
            presenter.present(ctx, &view, TuiRect::new(0, 0, 80, 16))
        });
        let lines = frame
            .buffer
            .to_lines()
            .into_iter()
            .map(|line| line.trim_end().to_owned())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        let (header_background, surface_background) = app.read(|ctx| {
            let builder = TuiUiBuilder::from_app(ctx);
            (
                builder.permission_header_background(),
                builder.permission_surface_background(),
            )
        });
        assert_ne!(header_background, surface_background);
        assert_eq!(frame.buffer[(79, 0)].bg, header_background);
        assert_eq!(frame.buffer[(79, 1)].bg, surface_background);
        let footer_row = frame
            .buffer
            .to_lines()
            .iter()
            .position(|line| line.contains("Esc to cancel"))
            .expect("permission footer");
        let footer_row = u16::try_from(footer_row).expect("footer row fits in the TUI");
        assert_eq!(frame.buffer[(79, footer_row)].bg, Color::Reset);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("■ Is it OK if I call MCP tool create_issue?"))
        );
        assert!(lines.iter().any(|line| line.contains("create_issue")));
        assert!(lines.iter().any(|line| line.contains("(1) yes")));
        assert!(lines.iter().any(|line| line.contains("(3) Other")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Esc to cancel  Enter to run"))
        );
    });
}

#[test]
fn accepting_new_conversation_suggestion_completes_the_executor() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|ctx| AppExecutionMode::new(ExecutionMode::App, false, ctx));
        let action_model = add_test_action_model(&mut app);
        let conversation_id = AIConversationId::new();
        let action = AIAgentAction {
            id: AIAgentActionId::from("suggest-conversation".to_owned()),
            task_id: TaskId::new("task".to_owned()),
            action: AIAgentActionType::SuggestNewConversation {
                message_id: "next-step".to_owned(),
            },
            requires_result: true,
        };
        let action_for_queue = action.clone();
        let action_model_for_view = action_model.clone();
        let view = app.update(|ctx| {
            let (window_id, _) = ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| TestHostView,
            );
            ctx.add_tui_view(window_id, |ctx| {
                TuiGenericToolCallView::new(
                    action,
                    false,
                    action_model_for_view,
                    conversation_id,
                    ctx,
                )
            })
        });
        action_model.update(&mut app, |model, ctx| {
            queue_tui_permission_action(model, action_for_queue, conversation_id, ctx);
        });
        // Let the spawned preprocessing land the action in the pending queue before
        // approving it; otherwise `accept` no-ops and no result is ever recorded.
        crate::test_fixtures::settle().await;

        view.update(&mut app, |view, ctx| view.accept(ctx));
        // Wait for the executor to reach a terminal result instead of awaiting a
        // `oneshot` fired from the `FinishedAction` subscription. The action
        // executes on the background executor (`ModelContext::spawn`) and its
        // result is handed back to the foreground, so a blocking `.await` here
        // took `async_io::block_on`'s park-for-notification path (its `Reactor`
        // lock is process-global and, under parallel test execution, held by
        // another thread) and depended on a cross-thread wake that was lost
        // intermittently — deadlocking the whole in-process suite at this test.
        // `settle_until` keeps `block_on` in its notified re-poll fast path while
        // briefly sleeping the thread so the background execution actually
        // completes and its result is delivered to the foreground.
        let result_id = AIAgentActionId::from("suggest-conversation".to_owned());
        let result_id_for_wait = result_id.clone();
        let action_model_for_wait = action_model.clone();
        crate::test_fixtures::settle_until(&mut app, |app| {
            app.read(|ctx| {
                action_model_for_wait
                    .as_ref(ctx)
                    .get_action_result(&result_id_for_wait)
                    .is_some()
            })
        })
        .await;

        app.read(|ctx| {
            let result = action_model
                .as_ref(ctx)
                .get_action_result(&result_id)
                .expect("suggestion result");
            assert!(matches!(
                &result.result,
                AIAgentActionResultType::SuggestNewConversation(
                    SuggestNewConversationResult::Accepted { message_id }
                ) if message_id == "next-step"
            ));
        });
    });
}
