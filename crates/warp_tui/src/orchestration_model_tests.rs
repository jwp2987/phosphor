//! Ported from the pinned oracle's `orchestration_model_tests.rs` (`02b53fcd8`).
//!
//! Three of the pin's five tests made the trip:
//! `snapshot_is_shared_across_tree_and_filters_conversations_without_sessions` (read-only
//! navigation, fed by conversations created directly through
//! `BlocklistAIHistoryModel::start_new_child_conversation`) and, new in this port,
//! `local_harness_children_fail_cleanly` / `failed_launch_cleanup_preserves_other_sessions` --
//! both exercise the now-built [`StartAgentExecutor`] / [`super::TuiOrchestrationModel::
//! dispatch_create_agent`] failure path. The remaining two
//! (`github_auth_blocker_keeps_the_remote_session_and_actionable_url`,
//! `remote_child_session_is_navigable_and_projects_lifecycle`) exercise the *remote*
//! child-session path -- declined cloud-runner orchestration, `DECLINED.md` #290 -- and are not
//! ported: this fork's `StartAgentExecutionMode` has no `Remote` variant to dispatch.
//!
//! The child-agent restoration block at the bottom came across with
//! `40ac1d4b1`; its own header records how the pin's cloud-seeded fixtures were
//! re-cut against local Oz children.

use warp::tui_export::{
    AIConversationId, BlocklistAIHistoryModel, Harness, StartAgentExecutionMode,
    StartAgentExecutor, StartAgentExecutorEvent, StartAgentOutcome,
    register_tui_session_view_test_singletons,
};
use warp_core::features::FeatureFlag;
use warpui::platform::WindowStyle;
use warpui::{AddWindowOptions, ModelHandle, ReadModel, SingletonEntity as _, UpdateModel};
use warpui_core::{App, WindowId};

use super::{ORCHESTRATOR_TAB_LABEL, TuiOrchestrationModel};
use crate::root_view::RootTuiView;
use crate::session_registry::{TuiSessionId, TuiSessionView, TuiSessions};
use crate::tab_bar::TuiTabBarNavigationDirection;
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
                model.set_explicit_page(parent_conversation_id, second_child_id, ctx);
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

/// Creates a standalone executor and relays its frontend materialization
/// events into the coordinator, mirroring how a real caller (once one
/// exists -- see [`super::TuiOrchestrationModel`]'s module doc) would wire
/// both `StartAgentExecutorEvent` variants to `dispatch_create_agent` /
/// `cleanup_child`.
fn add_relayed_executor(
    app: &mut App,
    parent_session_id: TuiSessionId,
) -> ModelHandle<StartAgentExecutor> {
    let executor = app.add_model(StartAgentExecutor::new);
    let executor_for_relay = executor.clone();
    app.update(|ctx| {
        let orchestration = TuiOrchestrationModel::handle(ctx);
        ctx.subscribe_to_model(&executor, move |_, event, ctx| {
            orchestration.update(ctx, |orchestration, ctx| match event {
                StartAgentExecutorEvent::CreateAgent(request) => {
                    orchestration.dispatch_create_agent(
                        parent_session_id,
                        (**request).clone(),
                        &executor_for_relay,
                        ctx,
                    );
                }
                StartAgentExecutorEvent::CleanupFailedChildLaunch { conversation_id } => {
                    orchestration.cleanup_child(conversation_id, ctx);
                }
            });
        });
    });
    executor
}

/// Dispatches a StartAgent request through the session's executor and
/// returns the resolved outcome. Every `Local` mode currently resolves
/// synchronously (within the same effect flush `dispatch` triggers), so the
/// receiver already has an answer by the time `update_model` returns.
fn dispatch_and_recv(
    app: &mut App,
    session_id: TuiSessionId,
    executor: &ModelHandle<StartAgentExecutor>,
    execution_mode: StartAgentExecutionMode,
) -> (AIConversationId, StartAgentOutcome) {
    let parent_conversation_id = app.read(|ctx| {
        BlocklistAIHistoryModel::as_ref(ctx)
            .active_conversation(session_id.surface_id())
            .expect("fixture registered an active conversation")
            .id()
    });
    let receiver = app.update_model(executor, |executor, ctx| {
        executor.dispatch(
            "researcher".to_string(),
            "research the codebase".to_string(),
            execution_mode,
            None,
            parent_conversation_id,
            Some("parent-run-1".to_string()),
            ctx,
        )
    });
    (
        parent_conversation_id,
        receiver
            .try_recv()
            .expect("unsupported-mode dispatches resolve before the update returns"),
    )
}

fn assert_error_containing(outcome: StartAgentOutcome, needle: &str) {
    match outcome {
        StartAgentOutcome::Error(message) => {
            assert!(message.contains(needle), "unexpected error: {message}");
        }
        StartAgentOutcome::Started { agent_id } => {
            panic!("expected an error outcome, got Started({agent_id})");
        }
    }
}

fn assert_failed_launch_cleaned_up(
    app: &App,
    fixture: &OrchestrationFixture,
    parent_conversation_id: AIConversationId,
    expected_session_count: usize,
) {
    app.read(|ctx| {
        let history = BlocklistAIHistoryModel::as_ref(ctx);
        assert!(
            history
                .child_conversation_ids_of(&parent_conversation_id)
                .is_empty()
        );
        assert!(
            TuiOrchestrationModel::as_ref(ctx)
                .event_consumers_by_session
                .is_empty()
        );
    });
    assert_eq!(
        app.read_model(&fixture.sessions, |sessions, _| sessions.len()),
        expected_session_count,
    );
}

#[test]
fn local_harness_children_fail_cleanly() {
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let session_id = add_dispatching_session(&mut app, &fixture, true);
        let executor = add_relayed_executor(&mut app, session_id);

        let (parent_conversation_id, outcome) = dispatch_and_recv(
            &mut app,
            session_id,
            &executor,
            StartAgentExecutionMode::Local {
                harness_type: Some("claude".to_string()),
                model_id: None,
            },
        );
        assert_error_containing(outcome, "aren't supported in Phosphor Agent CLI yet");
        assert_failed_launch_cleaned_up(&app, &fixture, parent_conversation_id, 1);
    });
}

#[test]
fn failed_launch_cleanup_preserves_other_sessions() {
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let _ = add_dispatching_session(&mut app, &fixture, true);
        let background_session_id = add_dispatching_session(&mut app, &fixture, false);
        let executor = add_relayed_executor(&mut app, background_session_id);

        let (parent_conversation_id, outcome) = dispatch_and_recv(
            &mut app,
            background_session_id,
            &executor,
            StartAgentExecutionMode::Local {
                harness_type: Some("codex".to_string()),
                model_id: None,
            },
        );
        assert_error_containing(outcome, "aren't supported in Phosphor Agent CLI yet");
        assert_failed_launch_cleaned_up(&app, &fixture, parent_conversation_id, 2);
    });
}

// ---- Child-agent restoration (APP-5038, `40ac1d4b1`) -----------------------
//
// The pin's restoration suite seeds its fixtures with `seed_remote_child` and
// asserts against the lightweight cloud session those children materialize
// into. That path is declined cloud-runner orchestration (`DECLINED.md` #290)
// and `TuiSessionView` has no `Cloud` variant here, so the fixtures below use
// local Oz children instead, and the two purely-cloud tests
// (`restored_remote_child_uses_authoritative_task_status`, and the remote half
// of `restore_skips_unsupported_or_malformed_children`) are covered only by
// their skip assertion.
//
// Like the pin's own `restored_local_oz_child_materializes_terminal_session_
// without_relaunch`, the tests that need a materialized child drive the
// restore-only pieces directly rather than going through
// `restore_descendant_sessions`: the local materialization path allocates a
// real PTY, which the pin also avoids doing in-process.

fn read_active_conversation_id(app: &App, session_id: TuiSessionId) -> AIConversationId {
    app.read(|ctx| {
        BlocklistAIHistoryModel::as_ref(ctx)
            .active_conversation(session_id.surface_id())
            .expect("session has an active conversation")
            .id()
    })
}

/// Seeds a hydrated local Oz child under `parent_conversation_id` with no
/// retained session, mimicking a startup-hydrated orchestration child that has
/// not yet been materialized.
fn seed_local_child(
    app: &mut App,
    parent_conversation_id: AIConversationId,
    name: &str,
) -> AIConversationId {
    app.update(|ctx| {
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            history.start_new_child_conversation(
                warpui::EntityId::new(),
                name.to_owned(),
                parent_conversation_id,
                Some(Harness::Oz),
                ctx,
            )
        })
    })
}

fn restore_descendants(
    app: &mut App,
    parent_conversation_id: AIConversationId,
    root_session_id: TuiSessionId,
) {
    app.update(|ctx| {
        TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
            model.restore_descendant_sessions(parent_conversation_id, root_session_id, ctx);
        });
    });
}

fn snapshot_child_ids(app: &App, selected: AIConversationId) -> Option<Vec<AIConversationId>> {
    app.read(|ctx| {
        TuiOrchestrationModel::as_ref(ctx)
            .snapshot(selected, ctx)
            .map(|snapshot| {
                snapshot
                    .children
                    .iter()
                    .map(|child| child.conversation_id)
                    .collect()
            })
    })
}

#[test]
fn restoring_parent_without_children_is_noop() {
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let parent_conversation_id = read_active_conversation_id(&app, parent_session_id);

        restore_descendants(&mut app, parent_conversation_id, parent_session_id);

        assert_eq!(
            app.read_model(&fixture.sessions, |sessions, _| sessions.len()),
            1
        );
        assert_eq!(snapshot_child_ids(&app, parent_conversation_id), None);
    });
}

#[test]
fn restoring_parent_twice_does_not_duplicate_child_sessions() {
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let parent_conversation_id = read_active_conversation_id(&app, parent_session_id);
        // Already materialized: the child owns a live session before restore.
        let (_child_session_id, child_id) =
            add_child_session(&mut app, &fixture, parent_conversation_id, "first-child");

        assert_eq!(
            app.read_model(&fixture.sessions, |sessions, _| sessions.len()),
            2
        );
        restore_descendants(&mut app, parent_conversation_id, parent_session_id);

        // Restoration is idempotent: an already-materialized descendant is
        // skipped, so no duplicate session or tab is created.
        assert_eq!(
            app.read_model(&fixture.sessions, |sessions, _| sessions.len()),
            2
        );
        assert_eq!(
            snapshot_child_ids(&app, parent_conversation_id),
            Some(vec![child_id])
        );
    });
}

#[test]
fn restore_skips_unsupported_or_malformed_children() {
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let parent_conversation_id = read_active_conversation_id(&app, parent_session_id);

        // An explicit local non-Oz harness child the TUI cannot display.
        let non_oz_id = app.update(|ctx| {
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.start_new_child_conversation(
                    warpui::EntityId::new(),
                    "claude-child".to_owned(),
                    parent_conversation_id,
                    Some(Harness::Claude),
                    ctx,
                )
            })
        });
        // A shared-session viewer child with no matching TUI view.
        let shared_viewer_id = seed_local_child(&mut app, parent_conversation_id, "shared-viewer");
        // A child flagged as running on a remote worker: declined cloud-runner
        // orchestration, so this fork has no session to restore it into.
        let remote_child_id = seed_local_child(&mut app, parent_conversation_id, "remote-child");
        app.update(|ctx| {
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, _ctx| {
                history
                    .conversation_mut(&shared_viewer_id)
                    .expect("shared viewer child exists")
                    .set_is_viewing_shared_session(true);
                history
                    .conversation_mut(&remote_child_id)
                    .expect("remote child exists")
                    .mark_as_remote_child();
            });
        });

        restore_descendants(&mut app, parent_conversation_id, parent_session_id);

        // Nothing materializes and the parent restore still succeeds.
        assert_eq!(
            app.read_model(&fixture.sessions, |sessions, _| sessions.len()),
            1
        );
        assert_eq!(snapshot_child_ids(&app, parent_conversation_id), None);

        // The skipped children keep their history records (nothing deleted).
        app.read(|ctx| {
            let history = BlocklistAIHistoryModel::as_ref(ctx);
            assert!(history.conversation(&non_oz_id).is_some());
            assert!(history.conversation(&shared_viewer_id).is_some());
            assert!(history.conversation(&remote_child_id).is_some());
        });
    });
}

#[test]
fn restored_local_oz_child_materializes_terminal_session_without_relaunch() {
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let parent_conversation_id = read_active_conversation_id(&app, parent_session_id);

        // Seed a hydrated local Oz child (no retained session yet).
        let child_id = seed_local_child(&mut app, parent_conversation_id, "local-child");

        // Materialize the child via the restore-only pieces on a fresh terminal
        // session: restore its transcript, then register it. This deliberately
        // does not call any launch/start-agent path, so the child is not
        // relaunched and no prompt is resent.
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
        let child_conversation = app.read(|ctx| {
            BlocklistAIHistoryModel::as_ref(ctx)
                .conversation(&child_id)
                .cloned()
                .expect("child conversation is hydrated")
        });
        app.update(|ctx| {
            child_view.update(ctx, |view, ctx| {
                view.restore_orchestrated_child_conversation(child_conversation, ctx);
            });
        });
        app.update(|ctx| {
            TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.register_restored_local_oz_child_session(child_session_id, child_id, ctx);
            });
        });

        app.read(|ctx| {
            // The child is a full terminal session.
            let session = TuiSessions::as_ref(ctx)
                .session(child_session_id)
                .expect("child session registered");
            assert!(matches!(session.view(), TuiSessionView::Terminal(_)));

            // It appears in the parent's orchestration snapshot with its
            // preserved agent name and parent linkage.
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(parent_conversation_id, ctx)
                .expect("child is navigable");
            let child = snapshot
                .children
                .iter()
                .find(|child| child.conversation_id == child_id)
                .expect("child has an orchestration tab");
            assert_eq!(child.label, "local-child");

            let history = BlocklistAIHistoryModel::as_ref(ctx);
            let conversation = history.conversation(&child_id).expect("child conversation");
            assert_eq!(
                history.resolved_parent_conversation_id_for_conversation(conversation),
                Some(parent_conversation_id)
            );
        });
    });
}

#[test]
fn discard_restored_descendant_sessions_removes_projections_without_deleting_records() {
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let parent_conversation_id = read_active_conversation_id(&app, parent_session_id);
        let (child_session_id, child_id) =
            add_child_session(&mut app, &fixture, parent_conversation_id, "local-child");
        app.update(|ctx| {
            TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.register_restored_local_oz_child_session(child_session_id, child_id, ctx);
            });
        });
        assert_eq!(
            app.read_model(&fixture.sessions, |sessions, _| sessions.len()),
            2
        );

        // When a different parent replaces the tree, the prior tree's restored
        // child-session projections are dropped without deleting the underlying
        // conversation.
        app.update(|ctx| {
            TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.discard_restored_descendant_sessions(parent_conversation_id, ctx);
            });
        });

        assert_eq!(
            app.read_model(&fixture.sessions, |sessions, _| sessions.len()),
            1
        );
        app.read(|ctx| {
            assert!(
                BlocklistAIHistoryModel::as_ref(ctx)
                    .conversation(&child_id)
                    .is_some(),
                "the child conversation record must be preserved after discard"
            );
        });
        assert_eq!(snapshot_child_ids(&app, parent_conversation_id), None);
    });
}

// ---------------------------------------------------------------------------
// Multi-level orchestration UI (upstream 683d40782, branch repin-orchui).
// Appended rather than interleaved: repin-orchrestore and repin-orchui each
// ported a complementary half of the same pin file, and two of git's hunks
// split mid-expression. Neither branch deleted a pre-existing test -- base
// had 10, orchrestore added 9, orchui added these 8.
// ---------------------------------------------------------------------------

/// The conversation a dispatching session is currently active on.
fn active_conversation_id(app: &App, session_id: TuiSessionId) -> AIConversationId {
    app.read(|ctx| {
        BlocklistAIHistoryModel::as_ref(ctx)
            .active_conversation(session_id.surface_id())
            .expect("active conversation")
            .id()
    })
}

#[test]
fn snapshot_keeps_flat_projection_with_multi_level_disabled() {
    let _flag = FeatureFlag::MultiLevelOrchestration.override_enabled(false);
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let parent_conversation_id = active_conversation_id(&app, parent_session_id);
        let (_, child_id) = add_child_session(&mut app, &fixture, parent_conversation_id, "child");
        let (_, grandchild_id) = add_child_session(&mut app, &fixture, child_id, "grandchild");

        // Flag off: every descendant renders as a flat sibling of the root
        // level, with no breadcrumbs, no anchor glyph, and no rollups.
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(grandchild_id, ctx)
                .expect("flat snapshot");
            assert_eq!(snapshot.anchor_conversation_id, parent_conversation_id);
            assert_eq!(snapshot.anchor_label, ORCHESTRATOR_TAB_LABEL);
            assert_eq!(snapshot.anchor_status, None);
            assert!(snapshot.breadcrumbs.is_empty());
            assert_eq!(
                snapshot
                    .children
                    .iter()
                    .map(|child| child.conversation_id)
                    .collect::<Vec<_>>(),
                vec![child_id, grandchild_id]
            );
            assert!(
                snapshot
                    .children
                    .iter()
                    .all(|child| child.subtree_rollup.is_none())
            );
        });
    });
}

#[test]
fn snapshot_renders_one_level_with_rollups_when_multi_level_is_enabled() {
    let _flag = FeatureFlag::MultiLevelOrchestration.override_enabled(true);
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let root_id = active_conversation_id(&app, parent_session_id);
        let (_, child_id) = add_child_session(&mut app, &fixture, root_id, "child");
        let (_, grandchild_id) = add_child_session(&mut app, &fixture, child_id, "grandchild");

        // The root level shows only the direct child, which carries a subtree
        // rollup for the grandchild.
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(root_id, ctx)
                .expect("root level snapshot");
            assert_eq!(snapshot.anchor_conversation_id, root_id);
            assert_eq!(snapshot.anchor_label, ORCHESTRATOR_TAB_LABEL);
            assert!(snapshot.anchor_status.is_some());
            assert!(snapshot.breadcrumbs.is_empty());
            assert_eq!(
                snapshot
                    .children
                    .iter()
                    .map(|child| child.conversation_id)
                    .collect::<Vec<_>>(),
                vec![child_id]
            );
            assert_eq!(
                snapshot.children[0]
                    .subtree_rollup
                    .as_ref()
                    .map(|rollup| rollup.descendant_count),
                Some(1),
                "the grandchild must roll up into the child's badge"
            );
        });

        // Selecting the group child re-anchors the bar to its level: the
        // grandchild becomes the row and a root breadcrumb leads back up.
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(child_id, ctx)
                .expect("drilled-in level snapshot");
            assert_eq!(snapshot.anchor_conversation_id, child_id);
            assert_eq!(snapshot.anchor_label, "child");
            assert_eq!(
                snapshot
                    .children
                    .iter()
                    .map(|child| child.conversation_id)
                    .collect::<Vec<_>>(),
                vec![grandchild_id]
            );
            assert_eq!(
                snapshot
                    .breadcrumbs
                    .iter()
                    .map(|breadcrumb| (breadcrumb.conversation_id, breadcrumb.label.as_str()))
                    .collect::<Vec<_>>(),
                vec![(root_id, ORCHESTRATOR_TAB_LABEL)]
            );
        });

        // A grandchild leaf anchors its parent's level (same row).
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(grandchild_id, ctx)
                .expect("leaf snapshot");
            assert_eq!(snapshot.anchor_conversation_id, child_id);
            assert_eq!(
                snapshot
                    .children
                    .iter()
                    .map(|child| child.conversation_id)
                    .collect::<Vec<_>>(),
                vec![grandchild_id]
            );
        });
    });
}

#[test]
fn breadcrumbs_cap_at_root_plus_parent_at_depth_three() {
    let _flag = FeatureFlag::MultiLevelOrchestration.override_enabled(true);
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let root_id = active_conversation_id(&app, parent_session_id);
        let (_, alpha_id) = add_child_session(&mut app, &fixture, root_id, "alpha");
        let (_, beta_id) = add_child_session(&mut app, &fixture, alpha_id, "beta");
        let (_, gamma_id) = add_child_session(&mut app, &fixture, beta_id, "gamma");

        // gamma is a leaf three levels down: the bar anchors its parent
        // (beta) and shows exactly two breadcrumbs — root plus parent's
        // parent — never the full ancestor chain.
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(gamma_id, ctx)
                .expect("depth-three snapshot");
            assert_eq!(snapshot.anchor_conversation_id, beta_id);
            assert_eq!(snapshot.anchor_label, "beta");
            assert_eq!(
                snapshot
                    .children
                    .iter()
                    .map(|child| child.conversation_id)
                    .collect::<Vec<_>>(),
                vec![gamma_id]
            );
            assert_eq!(
                snapshot
                    .breadcrumbs
                    .iter()
                    .map(|breadcrumb| (breadcrumb.conversation_id, breadcrumb.label.as_str()))
                    .collect::<Vec<_>>(),
                vec![(root_id, ORCHESTRATOR_TAB_LABEL), (alpha_id, "alpha")]
            );
        });
    });
}

#[test]
fn sessionless_parents_are_filtered_from_chips_and_marked_non_navigable() {
    let _flag = FeatureFlag::MultiLevelOrchestration.override_enabled(true);
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let root_session_id = add_dispatching_session(&mut app, &fixture, true);
        let root_id = active_conversation_id(&app, root_session_id);

        // A loaded but sessionless intermediate (e.g. a restored child whose
        // hidden session did not come back).
        let sessionless_parent_id = app.update(|ctx| {
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.start_new_child_conversation(
                    warpui::EntityId::new(),
                    "claude-mid".to_owned(),
                    root_id,
                    Some(Harness::Claude),
                    ctx,
                )
            })
        });
        let (_, session_backed_child_id) = add_child_session(
            &mut app,
            &fixture,
            sessionless_parent_id,
            "session-backed-child",
        );
        let (_, leaf_id) = add_child_session(&mut app, &fixture, session_backed_child_id, "leaf");

        app.read(|ctx| {
            let model = TuiOrchestrationModel::as_ref(ctx);
            // The leaf anchors its session-backed parent; the sessionless
            // grandparent contributes no breadcrumb chip — only the root
            // remains, so ascent stays reachable.
            let snapshot = model.snapshot(leaf_id, ctx).expect("leaf level snapshot");
            assert_eq!(snapshot.anchor_conversation_id, session_backed_child_id);
            assert!(snapshot.anchor_navigable);
            assert_eq!(
                snapshot
                    .breadcrumbs
                    .iter()
                    .map(|breadcrumb| breadcrumb.conversation_id)
                    .collect::<Vec<_>>(),
                vec![root_id],
                "a sessionless parent must not become a breadcrumb chip"
            );

            let snapshot = model
                .snapshot(session_backed_child_id, ctx)
                .expect("mid level snapshot");
            assert_eq!(snapshot.anchor_conversation_id, session_backed_child_id);
        });

        // Remove the leaf so session-backed-child becomes a leaf whose parent
        // is the sessionless conversation: the bar anchors the sessionless
        // parent and marks it non-navigable.
        app.update(|ctx| {
            TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.kill_child_agent(leaf_id, ctx);
            });
        });
        app.read(|ctx| {
            let snapshot = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(session_backed_child_id, ctx)
                .expect("sessionless-anchor snapshot");
            assert_eq!(snapshot.anchor_conversation_id, sessionless_parent_id);
            assert!(!snapshot.anchor_navigable);
            assert_eq!(
                snapshot
                    .breadcrumbs
                    .iter()
                    .map(|breadcrumb| breadcrumb.conversation_id)
                    .collect::<Vec<_>>(),
                vec![root_id]
            );
        });
    });
}

#[test]
fn adjacent_tree_conversation_walks_the_whole_tree_and_wraps() {
    let _flag = FeatureFlag::MultiLevelOrchestration.override_enabled(true);
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let root_id = active_conversation_id(&app, parent_session_id);
        let (_, child_id) = add_child_session(&mut app, &fixture, root_id, "child");
        let (_, grandchild_id) = add_child_session(&mut app, &fixture, child_id, "grandchild");

        app.read(|ctx| {
            let model = TuiOrchestrationModel::as_ref(ctx);
            // Tree order is root → child → grandchild, wrapping at the ends,
            // so Tab alone still reaches every agent at any depth.
            assert_eq!(
                model.adjacent_tree_conversation(root_id, TuiTabBarNavigationDirection::Next, ctx),
                Some(child_id)
            );
            assert_eq!(
                model.adjacent_tree_conversation(child_id, TuiTabBarNavigationDirection::Next, ctx),
                Some(grandchild_id)
            );
            assert_eq!(
                model.adjacent_tree_conversation(
                    grandchild_id,
                    TuiTabBarNavigationDirection::Next,
                    ctx
                ),
                Some(root_id)
            );
            assert_eq!(
                model.adjacent_tree_conversation(
                    root_id,
                    TuiTabBarNavigationDirection::Previous,
                    ctx
                ),
                Some(grandchild_id)
            );
        });
    });
}

#[test]
fn explicit_paging_is_tracked_per_level() {
    let _flag = FeatureFlag::MultiLevelOrchestration.override_enabled(true);
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let root_id = active_conversation_id(&app, parent_session_id);
        let (_, first_child_id) = add_child_session(&mut app, &fixture, root_id, "first-child");
        let (_, second_child_id) = add_child_session(&mut app, &fixture, root_id, "second-child");
        let (_, grandchild_id) =
            add_child_session(&mut app, &fixture, first_child_id, "grandchild");

        // Page explicitly within the root level.
        app.update(|ctx| {
            TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.set_explicit_page(root_id, second_child_id, ctx);
            });
        });
        app.read(|ctx| {
            let root_level = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(root_id, ctx)
                .expect("root level snapshot");
            assert_eq!(root_level.page_anchor, Some(second_child_id));
            assert!(!root_level.reveal_selected);
            // The drilled-in level under first-child is unaffected: it keeps
            // automatic reveal with its own first tab as the page anchor.
            let drilled = TuiOrchestrationModel::as_ref(ctx)
                .snapshot(first_child_id, ctx)
                .expect("drilled level snapshot");
            assert_eq!(drilled.anchor_conversation_id, first_child_id);
            assert_eq!(drilled.page_anchor, Some(grandchild_id));
            assert!(drilled.reveal_selected);
        });
    });
}

#[test]
fn kill_child_agent_subtree_removes_nested_descendants_with_the_child() {
    let _flag = FeatureFlag::MultiLevelOrchestration.override_enabled(true);
    App::test((), |mut app| async move {
        let fixture = orchestration_fixture(&mut app);
        let parent_session_id = add_dispatching_session(&mut app, &fixture, true);
        let parent_conversation_id = active_conversation_id(&app, parent_session_id);
        let (child_session_id, child_conversation_id) =
            add_child_session(&mut app, &fixture, parent_conversation_id, "researcher");
        let (grandchild_session_id, grandchild_conversation_id) = add_child_session(
            &mut app,
            &fixture,
            child_conversation_id,
            "nested-researcher",
        );

        // Killing the group child tears down the whole subtree deepest-first:
        // no orphaned grandchild session or conversation remains.
        app.update(|ctx| {
            TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                model.kill_child_agent_subtree(child_conversation_id, ctx);
            });
        });

        app.read(|ctx| {
            let history = BlocklistAIHistoryModel::as_ref(ctx);
            assert!(history.conversation(&child_conversation_id).is_none());
            assert!(history.conversation(&grandchild_conversation_id).is_none());
            assert!(
                history.conversation(&parent_conversation_id).is_some(),
                "the parent must survive the subtree kill"
            );
            let sessions = TuiSessions::as_ref(ctx);
            assert!(
                sessions
                    .session_id_for_surface(child_session_id.surface_id())
                    .is_none()
            );
            assert!(
                sessions
                    .session_id_for_surface(grandchild_session_id.surface_id())
                    .is_none()
            );
        });
        assert_eq!(
            app.read_model(&fixture.sessions, |sessions, _| sessions.len()),
            1
        );
    });
}
