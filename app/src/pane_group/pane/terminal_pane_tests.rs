//! Tests for `/orchestrate`'s display-path wiring: given a prepared local
//! child harness launch, does the child actually show up where the pill
//! bar, `ChildAgentStatusCard`, and transcript rendering (#304) look for it?
//!
//! `prepare_local_harness_child_launch` itself (the async half) is not
//! exercised here -- it shells out to `validate_cli_installed`, which
//! depends on the `claude`/`opencode` binary being on `PATH`, so it isn't
//! something a hermetic unit test can assert a success path for (see
//! `local_harness_launch_tests.rs`'s existing Codex-only coverage of that
//! function). These tests instead drive `finish_spawning_local_child_agent`
//! directly with a hand-built `PreparedLocalHarnessLaunch`, which is exactly
//! the seam between "the harness command was prepared" and "the app shows a
//! child agent" -- the part #325 actually asked to wire up.

use std::collections::HashMap;

use warpui::App;

use super::*;
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::pane_group::pane::local_harness_launch::PreparedLocalHarnessLaunch;

/// Builds a real `PaneGroup` with a single terminal pane. `finish_spawning_local_child_agent`
/// creates a genuine second terminal session (shell detection, PTY spawn) rather than a
/// mock, so this needs the fuller `Workspace`-level test harness -- `initialize_app_for_terminal_view`
/// alone panics on a missing `AvailableShells`/`RemoteServerManager` singleton. Reuses
/// `workspace::view::tests::initialize_app`/`mock_workspace`, the same harness
/// `crate::local_control::handlers::layout::tests` was already made to share (see the
/// `pub(crate) mod tests` doc comment on `workspace::view`).
fn pane_group_with_terminal(app: &mut App) -> (ViewHandle<PaneGroup>, PaneId) {
    crate::workspace::view::tests::initialize_app(app);
    let workspace = crate::workspace::view::tests::mock_workspace(app);

    let pane_group = workspace
        .read(app, |workspace, _| workspace.tab_views().next().cloned())
        .expect("mock_workspace has an initial tab");
    let base_pane_id = pane_group
        .read(app, |pane_group, _| pane_group.pane_id_from_index(0))
        .expect("initial tab has a pane at index 0");

    (pane_group, base_pane_id)
}

fn prepared_launch(command: &str) -> PreparedLocalHarnessLaunch {
    let task_id = AmbientAgentTaskId::new_local();
    PreparedLocalHarnessLaunch {
        command: command.to_string(),
        env_vars: HashMap::new(),
        run_id: task_id.to_string(),
        task_id,
    }
}

#[test]
fn finish_spawning_local_child_agent_wires_the_child_into_the_topology() {
    App::test((), |mut app| async move {
        let (pane_group, base_pane_id) = pane_group_with_terminal(&mut app);

        let parent_conversation_id = pane_group.update(&mut app, |group, ctx| {
            let base_terminal_view = group
                .terminal_view_from_pane_id(base_pane_id, ctx)
                .expect("base pane has a terminal view");
            let terminal_view_id = base_terminal_view.id();
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
                history_model.start_new_conversation(terminal_view_id, false, false, ctx)
            })
        });

        let prepared = prepared_launch("claude --session-id test 'write tests'");
        let child_id = pane_group.update(&mut app, |group, ctx| {
            finish_spawning_local_child_agent(
                group,
                base_pane_id,
                parent_conversation_id,
                "write tests".to_string(),
                prepared.clone(),
                ctx,
            );
            *group
                .child_agent_panes
                .keys()
                .next()
                .expect("a child pane was registered")
        });

        // The pill bar / status card display path: children_by_parent.
        pane_group.read(&app, |_group, ctx| {
            let history_model = BlocklistAIHistoryModel::as_ref(ctx);
            let children = history_model.child_conversations_of(parent_conversation_id);
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].id(), child_id);
            assert_eq!(children[0].agent_name(), Some("write tests"));
            assert_eq!(
                history_model.resolved_parent_conversation_id_for_conversation(children[0]),
                Some(parent_conversation_id)
            );
            // Wired via `assign_run_id_for_conversation`, matching the topology
            // index a nested `/orchestrate` would need to resolve its own parent.
            assert_eq!(children[0].agent_link_id(), Some(prepared.run_id.clone()));
        });

        // The PaneGroup display path: child_agent_panes + a real hidden pane.
        pane_group.read(&app, |group, ctx| {
            let child_pane_id = *group
                .child_agent_panes
                .get(&child_id)
                .expect("child_agent_panes has an entry for the spawned child");
            assert!(
                group.terminal_view_from_pane_id(child_pane_id, ctx).is_some(),
                "the child pane's terminal view must exist"
            );
        });
    });
}

#[test]
fn finish_spawning_local_child_agent_supports_multiple_children_under_one_parent() {
    App::test((), |mut app| async move {
        let (pane_group, base_pane_id) = pane_group_with_terminal(&mut app);

        let parent_conversation_id = pane_group.update(&mut app, |group, ctx| {
            let terminal_view_id = group
                .terminal_view_from_pane_id(base_pane_id, ctx)
                .expect("base pane has a terminal view")
                .id();
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
                history_model.start_new_conversation(terminal_view_id, false, false, ctx)
            })
        });

        // Mirrors what `spawn_local_child_agents` does per task after
        // `split_orchestrate_tasks("write tests; update the docs")`.
        for task in ["write tests", "update the docs"] {
            let prepared = prepared_launch(&format!("claude --session-id test '{task}'"));
            pane_group.update(&mut app, |group, ctx| {
                finish_spawning_local_child_agent(
                    group,
                    base_pane_id,
                    parent_conversation_id,
                    task.to_string(),
                    prepared,
                    ctx,
                );
            });
        }

        pane_group.read(&app, |group, ctx| {
            let history_model = BlocklistAIHistoryModel::as_ref(ctx);
            let mut names: Vec<_> = history_model
                .child_conversations_of(parent_conversation_id)
                .into_iter()
                .map(|c| c.agent_name().unwrap_or_default().to_string())
                .collect();
            names.sort();
            assert_eq!(names, vec!["update the docs", "write tests"]);
            assert_eq!(group.child_agent_panes.len(), 2);
        });
    });
}
