use std::collections::HashMap;

use warp::tui_export::{
    AIConversationId, AmbientAgentTaskId, BlocklistAIHistoryModel, Harness,
    TuiPreparedChildAgentLaunch, register_tui_session_view_test_singletons,
};
use warpui::platform::WindowStyle;
use warpui::{AddWindowOptions, App, ModelHandle};

use super::*;
use crate::orchestration_model::TuiOrchestrationModel;
use crate::root_view::RootTuiView;
use crate::test_fixtures::add_test_terminal_session;

struct PaneGroupTestFixture {
    window_id: warpui_core::WindowId,
    sessions: ModelHandle<TuiSessions>,
    pane_group: ModelHandle<TuiPaneGroup>,
    orchestration: ModelHandle<TuiOrchestrationModel>,
}

fn pane_group_test_fixture(app: &mut App) -> PaneGroupTestFixture {
    register_tui_session_view_test_singletons(app);
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
    let pane_group = app.update(TuiPaneGroup::register);
    PaneGroupTestFixture {
        window_id,
        sessions,
        pane_group,
        orchestration,
    }
}

/// Registers a mock (no real PTY) terminal session and a top-level
/// conversation on it, mirroring
/// `terminal_session_view_tests::add_orchestration_session`.
fn add_parent_session(
    app: &mut App,
    fixture: &PaneGroupTestFixture,
) -> (TuiSessionId, AIConversationId) {
    let (view, manager) = add_test_terminal_session(app, fixture.window_id);
    let session_id = app
        .update(|ctx| TuiSessions::register_session(&fixture.sessions, view, manager, true, ctx));
    let conversation_id = app.update(|ctx| {
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            let conversation_id =
                history.start_new_conversation(session_id.surface_id(), false, false, ctx);
            history.set_active_conversation_id(conversation_id, session_id.surface_id(), ctx);
            conversation_id
        })
    });
    (session_id, conversation_id)
}

fn fake_prepared_launch(command: &str) -> TuiPreparedChildAgentLaunch {
    TuiPreparedChildAgentLaunch {
        command: command.to_owned(),
        env_vars: HashMap::new(),
        run_id: "test-run-id".to_owned(),
        task_id: AmbientAgentTaskId::new_local(),
    }
}

/// [`TuiPaneGroup::finish_spawning_local_child_agent`] is the half of child
/// materialization that has TUI-specific behavior (the other half --
/// actually creating a real PTY-backed session -- is `create_local_terminal_session_with_env`,
/// exercised only by the (untested, per project convention for real-PTY
/// paths) `spawn_local_child_agents` caller). This test drives it directly
/// against a mock session, the same shortcut
/// `terminal_session_view_tests::add_orchestration_child` takes for
/// non-agent-launched children, and checks all three things the GUI's
/// `finish_spawning_local_child_agent` guarantees: the child conversation is
/// registered under the *child's own* session, it is tracked in
/// `child_agent_sessions`, and it is reachable from the orchestration
/// snapshot the tab bar renders from.
#[test]
fn finish_spawning_local_child_agent_registers_and_tracks_child() {
    App::test((), |mut app| async move {
        let fixture = pane_group_test_fixture(&mut app);
        let (_parent_session_id, parent_conversation_id) = add_parent_session(&mut app, &fixture);

        let (child_view, child_manager) = add_test_terminal_session(&mut app, fixture.window_id);
        let child_session_id = app.update(|ctx| {
            TuiSessions::register_session(
                &fixture.sessions,
                child_view.clone(),
                child_manager,
                false,
                ctx,
            )
        });
        let prepared = fake_prepared_launch("claude --dangerously-skip-permissions 'do the thing'");

        let child_id = app.update(|ctx| {
            fixture.pane_group.update(ctx, |pane_group, ctx| {
                pane_group.finish_spawning_local_child_agent(
                    child_session_id,
                    &child_view,
                    parent_conversation_id,
                    "do the thing".to_owned(),
                    &prepared,
                    ctx,
                )
            })
        });

        // Tracked for later lookup/discard.
        app.read(|ctx| {
            assert_eq!(
                fixture
                    .pane_group
                    .as_ref(ctx)
                    .session_for_conversation(child_id),
                Some(child_session_id),
                "the child conversation must be tracked against its own session"
            );
        });

        // Registered under its OWN session, not the parent's -- required for
        // `TuiSessions::session_ids_by_conversation` (and therefore the
        // orchestration snapshot) to resolve it.
        app.read(|ctx| {
            let history = BlocklistAIHistoryModel::as_ref(ctx);
            let conversation = history
                .conversation(&child_id)
                .expect("child conversation must be registered");
            assert_eq!(conversation.orchestration_harness(), Some(Harness::Claude));
            let sessions = fixture.sessions.as_ref(ctx);
            let session_ids_by_conversation = sessions.session_ids_by_conversation(history);
            assert_eq!(
                session_ids_by_conversation.get(&child_id).copied(),
                Some(child_session_id)
            );
        });

        // Real, end-to-end proof this closes the original gap: the
        // orchestration tab bar's snapshot -- the same read path
        // `TuiOrchestrationModel::snapshot` production code and the pinned
        // orchestration tab bar tests use -- now finds a real child.
        app.read(|ctx| {
            let orchestration = fixture.orchestration.as_ref(ctx);
            let snapshot = orchestration
                .snapshot(parent_conversation_id, ctx)
                .expect("materialized child must appear in the orchestration snapshot");
            assert_eq!(snapshot.root_conversation_id, parent_conversation_id);
            assert!(
                snapshot
                    .children
                    .iter()
                    .any(|child| child.conversation_id == child_id),
                "materialized child must be listed in the snapshot's children"
            );
        });
    });
}

#[test]
fn discard_child_agent_session_for_conversation_removes_tracking_and_session() {
    App::test((), |mut app| async move {
        let fixture = pane_group_test_fixture(&mut app);
        let (_parent_session_id, parent_conversation_id) = add_parent_session(&mut app, &fixture);

        let (child_view, child_manager) = add_test_terminal_session(&mut app, fixture.window_id);
        let child_session_id = app.update(|ctx| {
            TuiSessions::register_session(
                &fixture.sessions,
                child_view.clone(),
                child_manager,
                false,
                ctx,
            )
        });
        let prepared = fake_prepared_launch("claude --dangerously-skip-permissions 'task'");
        let child_id = app.update(|ctx| {
            fixture.pane_group.update(ctx, |pane_group, ctx| {
                pane_group.finish_spawning_local_child_agent(
                    child_session_id,
                    &child_view,
                    parent_conversation_id,
                    "task".to_owned(),
                    &prepared,
                    ctx,
                )
            })
        });

        let discarded = app.update(|ctx| {
            fixture.pane_group.update(ctx, |pane_group, ctx| {
                pane_group.discard_child_agent_session_for_conversation(
                    &fixture.sessions,
                    child_id,
                    ctx,
                )
            })
        });
        assert!(discarded, "a tracked child session must report discarded");

        app.read(|ctx| {
            assert_eq!(
                fixture
                    .pane_group
                    .as_ref(ctx)
                    .session_for_conversation(child_id),
                None,
                "tracking must be removed once discarded"
            );
            assert!(
                fixture
                    .sessions
                    .as_ref(ctx)
                    .session(child_session_id)
                    .is_none(),
                "the hidden session itself must be removed"
            );
        });

        let discarded_again = app.update(|ctx| {
            fixture.pane_group.update(ctx, |pane_group, ctx| {
                pane_group.discard_child_agent_session_for_conversation(
                    &fixture.sessions,
                    child_id,
                    ctx,
                )
            })
        });
        assert!(
            !discarded_again,
            "discarding an untracked conversation must report false, not panic"
        );
    });
}

/// A killed/deleted child conversation must not keep occupying its tracked
/// slot in `child_agent_sessions` -- otherwise a later conversation id reuse
/// (unlikely but not impossible) or a stale `session_for_conversation` read
/// could resolve to a session that no longer represents that conversation.
#[test]
fn removed_conversation_prunes_tracking_without_touching_the_session() {
    App::test((), |mut app| async move {
        let fixture = pane_group_test_fixture(&mut app);
        let (_parent_session_id, parent_conversation_id) = add_parent_session(&mut app, &fixture);

        let (child_view, child_manager) = add_test_terminal_session(&mut app, fixture.window_id);
        let child_session_id = app.update(|ctx| {
            TuiSessions::register_session(
                &fixture.sessions,
                child_view.clone(),
                child_manager,
                false,
                ctx,
            )
        });
        let prepared = fake_prepared_launch("claude --dangerously-skip-permissions 'task'");
        let child_id = app.update(|ctx| {
            fixture.pane_group.update(ctx, |pane_group, ctx| {
                pane_group.finish_spawning_local_child_agent(
                    child_session_id,
                    &child_view,
                    parent_conversation_id,
                    "task".to_owned(),
                    &prepared,
                    ctx,
                )
            })
        });

        app.update(|ctx| {
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.delete_conversation(child_id, None, ctx);
            });
        });

        app.read(|ctx| {
            assert_eq!(
                fixture
                    .pane_group
                    .as_ref(ctx)
                    .session_for_conversation(child_id),
                None,
                "a deleted conversation's tracking entry must be pruned"
            );
        });
    });
}
