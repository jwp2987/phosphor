//! Ported from the pinned oracle's `orchestration_model_tests.rs` (`02b53fcd8`).
//!
//! Only `snapshot_is_shared_across_tree_and_filters_conversations_without_sessions`
//! made the trip. The other four pin tests in this file
//! (`local_harness_children_fail_cleanly`, `failed_launch_cleanup_preserves_other_sessions`,
//! `github_auth_blocker_keeps_the_remote_session_and_actionable_url`,
//! `remote_child_session_is_navigable_and_projects_lifecycle`) all exercise child-session
//! *materialization* (`StartAgentExecutor`, `StartAgentRequest`, remote/cloud-runner launch),
//! which [`super::TuiOrchestrationModel`]'s module doc explains was deliberately cut, not
//! stubbed -- see that comment for the full rationale (local half: future work; remote half:
//! cloud-runner orchestration, out of scope pending #290). This test exercises exactly the
//! read-only navigation surface the fork kept: `snapshot`, `focus_conversation_session`, and
//! `set_explicit_page`, fed by conversations created directly through
//! `BlocklistAIHistoryModel::start_new_child_conversation` rather than through the cut executor.

use warp::tui_export::{
    AIConversationId, BlocklistAIHistoryModel, Harness, register_tui_session_view_test_singletons,
};
use warpui::platform::WindowStyle;
use warpui::{AddWindowOptions, ModelHandle, ReadModel, SingletonEntity as _, UpdateModel};
use warpui_core::{App, WindowId};

use super::TuiOrchestrationModel;
use crate::root_view::RootTuiView;
use crate::session_registry::{TuiSessionId, TuiSessions};
use crate::test_fixtures::{add_test_semantic_selection, add_test_terminal_session};

struct OrchestrationFixture {
    sessions: ModelHandle<TuiSessions>,
    window_id: WindowId,
}

/// Boots the container + root + orchestration model wiring (no live PTYs).
fn orchestration_fixture(app: &mut App) -> OrchestrationFixture {
    register_tui_session_view_test_singletons(app);
    app.update(|ctx| add_test_semantic_selection(ctx));
    app.update(crate::autoupdate::TuiAutoupdater::register);
    let (window_id, _root) = app.update(|ctx| {
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
    OrchestrationFixture {
        sessions,
        window_id,
    }
}

fn add_dispatching_session(
    app: &mut App,
    fixture: &OrchestrationFixture,
    focus: bool,
) -> TuiSessionId {
    let (session, manager) = add_test_terminal_session(app, fixture.window_id);
    let session_id = app.update(|ctx| {
        TuiSessions::register_session(&fixture.sessions, session, manager, focus, ctx)
    });
    app.update(|ctx| {
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            let conversation_id =
                history.start_new_conversation(session_id.surface_id(), false, false, ctx);
            history.set_active_conversation_id(conversation_id, session_id.surface_id(), ctx);
        });
    });
    session_id
}

fn add_child_session(
    app: &mut App,
    fixture: &OrchestrationFixture,
    parent_conversation_id: AIConversationId,
    name: &str,
) -> (TuiSessionId, AIConversationId) {
    let (session, manager) = add_test_terminal_session(app, fixture.window_id);
    let session_id = app.update(|ctx| {
        TuiSessions::register_session(&fixture.sessions, session, manager, false, ctx)
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
    (session_id, conversation_id)
}

#[test]
fn snapshot_is_shared_across_tree_and_filters_conversations_without_sessions() {
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let parent_conversation_id = app.read(|ctx| {
            BlocklistAIHistoryModel::as_ref(ctx)
                .active_conversation(parent_session_id.surface_id())
                .expect("parent conversation")
                .id()
        });
        let (first_session_id, first_child_id) =
            add_child_session(&mut app, &fixture, parent_conversation_id, "first-child");
        let (second_session_id, second_child_id) =
            add_child_session(&mut app, &fixture, parent_conversation_id, "second-child");
        app.update(|ctx| {
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.start_new_child_conversation(
                    warpui::EntityId::new(),
                    "missing-session".to_owned(),
                    parent_conversation_id,
                    Some(Harness::Oz),
                    ctx,
                );
            });
        });

        app.read(|ctx| {
            let model = TuiOrchestrationModel::as_ref(ctx);
            let parent = model
                .snapshot(parent_conversation_id, ctx)
                .expect("parent has navigable children");
            let child = model
                .snapshot(first_child_id, ctx)
                .expect("child resolves the same tree");
            assert_eq!(parent.root_conversation_id, parent_conversation_id);
            assert_eq!(child.root_conversation_id, parent_conversation_id);
            assert_eq!(
                parent
                    .children
                    .iter()
                    .map(|child| child.conversation_id)
                    .collect::<Vec<_>>(),
                vec![first_child_id, second_child_id]
            );
            assert_eq!(
                parent
                    .children
                    .iter()
                    .map(|child| child.spawn_index)
                    .collect::<Vec<_>>(),
                vec![0, 1]
            );
        });
        app.update(|ctx| {
            let selected = TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.focus_conversation_session(second_child_id, ctx)
            });
            assert_eq!(selected, Some(second_session_id));
        });
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(second_child_id, ctx)
                .expect("tab snapshot");
            assert_eq!(snapshot.page_anchor, Some(first_child_id));
            assert!(snapshot.reveal_selected);
        });
        app.update(|ctx| {
            TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.set_explicit_page(second_child_id, ctx);
            });
        });
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(parent_conversation_id, ctx)
                .expect("tab snapshot");
            assert_eq!(snapshot.page_anchor, Some(second_child_id));
            assert!(!snapshot.reveal_selected);
        });

        app.update(|ctx| {
            let selected = TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.focus_conversation_session(first_child_id, ctx)
            });
            assert_eq!(selected, Some(first_session_id));
        });
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(first_child_id, ctx)
                .expect("tab snapshot");
            assert_eq!(
                TuiSessions::as_ref(ctx).focused_session_id(),
                Some(first_session_id)
            );
            assert_eq!(snapshot.page_anchor, Some(first_child_id));
            assert!(snapshot.reveal_selected);
        });
    });
}
