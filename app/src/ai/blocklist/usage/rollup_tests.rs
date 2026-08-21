//! Ported from the pin's `app/src/ai/blocklist/usage/rollup_tests.rs`
//! (`02b53fcd8`), adapted to this fork's `BlocklistAIHistoryModel::
//! start_new_conversation`, which takes two bools
//! (`is_autoexecute_override`, `is_viewing_shared_session`), not the pin's
//! three. Setup otherwise mirrors `orchestration_topology_tests.rs`'s
//! `descendant_conversation_ids_in_spawn_order_flattens_nested_children_preorder`,
//! which exercises this same rollup's sole dependency without needing the
//! settings/persistence singletons — this module's functions don't reach
//! the persistence path either.

use warpui::{App, EntityId, ModelHandle};

use super::*;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::BlocklistAIHistoryModel;

fn set_credits(
    app: &mut App,
    history: &ModelHandle<BlocklistAIHistoryModel>,
    id: AIConversationId,
    credits: f32,
) {
    history.update(app, |history, _| {
        history
            .conversation_mut(&id)
            .expect("conversation must be loaded")
            .set_credits_spent_for_test(credits);
    });
}

fn spawn_child(
    app: &mut App,
    history: &ModelHandle<BlocklistAIHistoryModel>,
    name: &str,
    parent_id: AIConversationId,
    terminal_view_id: EntityId,
) -> AIConversationId {
    history.update(app, |history, ctx| {
        history.start_new_child_conversation(
            terminal_view_id,
            name.to_string(),
            parent_id,
            None,
            ctx,
        )
    })
}

#[test]
fn returns_none_when_orchestrator_has_no_descendants() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        // Even if the orchestrator itself has spent credits, no descendants
        // means no rollup applies.
        set_credits(&mut app, &history, orchestrator_id, 10.0);

        history.read(&app, |history, _| {
            assert!(compute_orchestration_rollup(orchestrator_id, history).is_none());
        });
    });
}

#[test]
fn sums_orchestrator_and_loaded_descendants() {
    App::test((), |mut app| async move {
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Register both so the persist path has the
        // singletons it legitimately needs.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_id = spawn_child(
            &mut app,
            &history,
            "DesignBot",
            orchestrator_id,
            terminal_view_id,
        );

        set_credits(&mut app, &history, orchestrator_id, 3.0);
        set_credits(&mut app, &history, child_id, 30.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.total_credits, 33.0);
            assert_eq!(rollup.per_agent.len(), 2);
            // Child spent more, sorted first.
            assert_eq!(rollup.per_agent[0].conversation_id, child_id);
            assert_eq!(rollup.per_agent[0].credits_spent, 30.0);
            assert_eq!(rollup.per_agent[0].avatar, AgentAvatar::Child);
            assert_eq!(rollup.per_agent[0].display_name, "DesignBot");
            assert_eq!(rollup.per_agent[1].conversation_id, orchestrator_id);
            assert_eq!(rollup.per_agent[1].credits_spent, 3.0);
            assert_eq!(rollup.per_agent[1].avatar, AgentAvatar::Orchestrator);
            assert_eq!(rollup.per_agent[1].display_name, "Orchestrator");
        });
    });
}

#[test]
fn excludes_zero_credit_descendants_from_breakdown() {
    App::test((), |mut app| async move {
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Register both so the persist path has the
        // singletons it legitimately needs.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let alpha_id = spawn_child(
            &mut app,
            &history,
            "Alpha",
            orchestrator_id,
            terminal_view_id,
        );
        let beta_id = spawn_child(
            &mut app,
            &history,
            "Beta",
            orchestrator_id,
            terminal_view_id,
        );
        let _idle_id = spawn_child(
            &mut app,
            &history,
            "IdleChild",
            orchestrator_id,
            terminal_view_id,
        );

        set_credits(&mut app, &history, orchestrator_id, 2.0);
        set_credits(&mut app, &history, alpha_id, 12.0);
        set_credits(&mut app, &history, beta_id, 5.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.total_credits, 19.0);
            assert_eq!(rollup.per_agent.len(), 3);
            let ordered_ids: Vec<_> = rollup
                .per_agent
                .iter()
                .map(|entry| entry.conversation_id)
                .collect();
            assert_eq!(ordered_ids, vec![alpha_id, beta_id, orchestrator_id]);
        });
    });
}

#[test]
fn rolls_up_grandchildren_transitively() {
    App::test((), |mut app| async move {
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Register both so the persist path has the
        // singletons it legitimately needs.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_id = spawn_child(
            &mut app,
            &history,
            "ChildA",
            orchestrator_id,
            terminal_view_id,
        );
        let grandchild_id = spawn_child(&mut app, &history, "GrandA1", child_id, terminal_view_id);

        set_credits(&mut app, &history, orchestrator_id, 1.0);
        set_credits(&mut app, &history, child_id, 4.0);
        set_credits(&mut app, &history, grandchild_id, 9.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.total_credits, 14.0);
            let ordered_ids: Vec<_> = rollup
                .per_agent
                .iter()
                .map(|entry| entry.conversation_id)
                .collect();
            assert_eq!(ordered_ids, vec![grandchild_id, child_id, orchestrator_id]);
        });
    });
}

#[test]
fn returns_six_contributors_for_show_n_more_caller() {
    App::test((), |mut app| async move {
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Register both so the persist path has the
        // singletons it legitimately needs.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        set_credits(&mut app, &history, orchestrator_id, 1.0);

        for i in 0..5 {
            let id = spawn_child(
                &mut app,
                &history,
                &format!("Agent{i}"),
                orchestrator_id,
                terminal_view_id,
            );
            // Distinct credit values so we don't rely on tie-break behavior.
            set_credits(&mut app, &history, id, (10 + i) as f32);
        }

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.per_agent.len(), 6);
        });
    });
}

#[test]
fn returns_none_when_only_orchestrator_has_zero_credits_with_loaded_children() {
    App::test((), |mut app| async move {
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Register both so the persist path has the
        // singletons it legitimately needs.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        // One spawned child, but neither it nor the orchestrator has spent
        // any credits yet.
        let _child_id = spawn_child(
            &mut app,
            &history,
            "Idle",
            orchestrator_id,
            terminal_view_id,
        );

        history.read(&app, |history, _| {
            assert!(compute_orchestration_rollup(orchestrator_id, history).is_none());
        });
    });
}

#[test]
fn ties_break_by_spawn_order_earlier_first() {
    App::test((), |mut app| async move {
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Register both so the persist path has the
        // singletons it legitimately needs.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let first_id = spawn_child(
            &mut app,
            &history,
            "FirstSpawned",
            orchestrator_id,
            terminal_view_id,
        );
        let second_id = spawn_child(
            &mut app,
            &history,
            "SecondSpawned",
            orchestrator_id,
            terminal_view_id,
        );

        // Equal credit values force a tie-break.
        set_credits(&mut app, &history, first_id, 7.0);
        set_credits(&mut app, &history, second_id, 7.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.per_agent.len(), 2);
            assert_eq!(rollup.per_agent[0].conversation_id, first_id);
            assert_eq!(rollup.per_agent[1].conversation_id, second_id);
        });
    });
}

#[test]
fn unloaded_descendant_id_is_silently_skipped() {
    App::test((), |mut app| async move {
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Register both so the persist path has the
        // singletons it legitimately needs.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let real_child_id = spawn_child(
            &mut app,
            &history,
            "RealChild",
            orchestrator_id,
            terminal_view_id,
        );
        set_credits(&mut app, &history, real_child_id, 4.0);

        // Manually insert a dangling parent -> child mapping for an ID that
        // is not present in `conversations_by_id`. This emulates an
        // orchestration topology entry where the child's `AIConversation`
        // hasn't been hydrated locally (e.g. a not-yet-restored child).
        let unloaded_id = AIConversationId::new();
        history.update(&mut app, |history, _| {
            history.set_parent_for_conversation(unloaded_id, orchestrator_id);
        });

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.total_credits, 4.0);
            assert_eq!(rollup.per_agent.len(), 1);
            assert_eq!(rollup.per_agent[0].conversation_id, real_child_id);
        });
    });
}

// ---------------------------------------------------------------------------
// Arithmetic and aggregation. This fork is BYOP: the number this rollup
// produces is the number a user reconciles against their own provider
// invoice, so the tests below are about the sum being right, not about the
// rows looking right.
// ---------------------------------------------------------------------------

/// "Present but zero" must not be conflated with "absent". Idle children keep
/// the rollup alive (there ARE descendants), contribute nothing to the total,
/// and stay out of the breakdown — leaving the orchestrator as the sole row.
#[test]
fn idle_children_keep_the_rollup_alive_without_appearing_or_adding_credit() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let idle_a = spawn_child(&mut app, &history, "IdleA", orchestrator_id, terminal_view_id);
        let idle_b = spawn_child(&mut app, &history, "IdleB", orchestrator_id, terminal_view_id);

        set_credits(&mut app, &history, orchestrator_id, 2.5);
        // Explicitly zero, not merely left unset: a child that ran and spent
        // nothing must behave the same as one that never started.
        set_credits(&mut app, &history, idle_a, 0.0);
        set_credits(&mut app, &history, idle_b, 0.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("descendants exist and the orchestrator spent credits");
            assert_eq!(rollup.total_credits, 2.5);
            assert_eq!(
                rollup.per_agent.len(),
                1,
                "zero-credit children must not occupy a breakdown row"
            );
            assert_eq!(rollup.per_agent[0].conversation_id, orchestrator_id);
            assert_eq!(rollup.per_agent[0].avatar, AgentAvatar::Orchestrator);
        });
    });
}

/// Credits are fractional under BYOP, so the sum has to hold for values that
/// are not whole numbers. Compared with a tolerance because the sum of three
/// one-decimal `f32`s is not bit-identical to the `f32` nearest their exact
/// total — the point of the test is the arithmetic, not `f32`'s last bit.
#[test]
fn fractional_credits_are_summed_across_the_whole_tree() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_a = spawn_child(&mut app, &history, "Alpha", orchestrator_id, terminal_view_id);
        let child_b = spawn_child(&mut app, &history, "Beta", orchestrator_id, terminal_view_id);

        set_credits(&mut app, &history, orchestrator_id, 0.5);
        set_credits(&mut app, &history, child_a, 0.3);
        set_credits(&mut app, &history, child_b, 0.1);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert!(
                (rollup.total_credits - 0.9).abs() < 1e-6,
                "0.5 + 0.3 + 0.1 should total 0.9, got {}",
                rollup.total_credits
            );
            let ordered: Vec<_> = rollup
                .per_agent
                .iter()
                .map(|entry| entry.conversation_id)
                .collect();
            assert_eq!(ordered, vec![orchestrator_id, child_a, child_b]);
        });
    });
}

/// **Documents a real accounting limit, it does not endorse it.**
///
/// [`AIConversation::credits_spent`] rounds to one decimal
/// (`(credits * 10.0).round() / 10.0`) *before* this rollup ever sees the
/// value, so spend below 0.05 credits per conversation is rounded away
/// per-agent and never reaches the sum. Three agents that each really spent
/// 0.04 credits (0.12 in total) therefore report as "nothing spent at all"
/// and the footer's "View details" never appears.
///
/// This is not a fork divergence — the pin rounds in the same place
/// (`42effe840:app/src/ai/agent/conversation.rs:769-773`) — so it is pinned
/// here rather than changed unilaterally. Fixing it means summing the raw
/// `conversation_usage_metadata.credits_spent` and rounding once at render
/// time, which is a behaviour change worth its own issue.
#[test]
fn spend_below_half_a_tenth_of_a_credit_is_rounded_away_before_it_is_summed() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        for i in 0..3 {
            let id = spawn_child(
                &mut app,
                &history,
                &format!("Frugal{i}"),
                orchestrator_id,
                terminal_view_id,
            );
            set_credits(&mut app, &history, id, 0.04);
        }

        history.read(&app, |history, _| {
            assert!(
                compute_orchestration_rollup(orchestrator_id, history).is_none(),
                "0.12 credits of real spend currently rounds away to nothing"
            );
        });
    });
}

/// The orchestrator's own row prefers its assigned agent name; only an
/// absent or empty name falls back to the literal "Orchestrator".
#[test]
fn a_named_orchestrator_row_uses_its_agent_name() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let _child_id = spawn_child(
            &mut app,
            &history,
            "Worker",
            orchestrator_id,
            terminal_view_id,
        );
        history.update(&mut app, |history, _| {
            history
                .conversation_mut(&orchestrator_id)
                .expect("orchestrator is loaded")
                .set_agent_name("Coordinator".to_string());
        });
        set_credits(&mut app, &history, orchestrator_id, 6.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.per_agent[0].display_name, "Coordinator");
        });
    });
}

/// An agent spawned before it has been named carries an empty `agent_name`,
/// which must render as "Agent" rather than as a blank row — the empty-string
/// case is what the `filter(|n| !n.is_empty())` in `child_display_name`
/// exists for, and a plain `unwrap_or` would silently ship the blank.
#[test]
fn an_unnamed_child_row_falls_back_to_agent() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let unnamed_id = spawn_child(&mut app, &history, "", orchestrator_id, terminal_view_id);
        set_credits(&mut app, &history, unnamed_id, 3.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.per_agent[0].conversation_id, unnamed_id);
            assert_eq!(rollup.per_agent[0].display_name, "Agent");
        });
    });
}

/// A conversation id that was never loaded and has no topology entry must
/// return `None` rather than panic — the footer asks for a rollup on every
/// render, including for conversations that have just been closed.
#[test]
fn an_unknown_conversation_id_has_no_rollup() {
    App::test((), |app| async move {
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let stranger_id = AIConversationId::new();

        history.read(&app, |history, _| {
            assert!(compute_orchestration_rollup(stranger_id, history).is_none());
        });
    });
}

/// The mirror image of `unloaded_descendant_id_is_silently_skipped`: the
/// *orchestrator* is the one not hydrated, while its children are. The
/// `if let Some(orchestrator)` arm is skipped, and the children's credits
/// must still be summed rather than the whole rollup collapsing to `None`.
#[test]
fn an_unloaded_orchestrator_still_sums_its_loaded_children() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        // A loaded conversation re-pointed at a parent that was never
        // hydrated into `conversations_by_id`.
        let child_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        set_credits(&mut app, &history, child_id, 8.0);

        let unloaded_orchestrator_id = AIConversationId::new();
        history.update(&mut app, |history, _| {
            history.set_parent_for_conversation(child_id, unloaded_orchestrator_id);
        });

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(unloaded_orchestrator_id, history)
                .expect("a loaded child is enough for a rollup");
            assert_eq!(rollup.total_credits, 8.0);
            assert_eq!(rollup.per_agent.len(), 1);
            assert_eq!(rollup.per_agent[0].conversation_id, child_id);
            assert_eq!(rollup.per_agent[0].avatar, AgentAvatar::Child);
        });
    });
}

/// Indexing the same child under the same parent twice must not put it in the
/// descendant list twice. If it ever did, its credits would be added to
/// `total_credits` twice and it would occupy two breakdown rows — the exact
/// double-count this rollup must never produce. Both index writers
/// (`set_parent_for_conversation` and the restore path) guard with a
/// `contains` check; this pins the consequence at the rollup level, where a
/// regression in either would actually be paid for.
#[test]
fn re_indexing_a_child_under_the_same_parent_does_not_double_count_it() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_id = spawn_child(
            &mut app,
            &history,
            "Worker",
            orchestrator_id,
            terminal_view_id,
        );
        set_credits(&mut app, &history, orchestrator_id, 1.0);
        set_credits(&mut app, &history, child_id, 9.0);

        // Re-establish the same edge, as a restore over an already-spawned
        // child would.
        history.update(&mut app, |history, _| {
            history.set_parent_for_conversation(child_id, orchestrator_id);
        });

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.total_credits, 10.0, "the child must be counted once");
            assert_eq!(rollup.per_agent.len(), 2);
            assert_eq!(rollup.per_agent[0].conversation_id, child_id);
        });
    });
}

/// Ties break by position in the pre-order descendant walk, not by depth and
/// not by branch. `Grandchild` (spawned under `Alpha`, so pre-order position
/// 2) must sort ahead of `Beta` (a later sibling of `Alpha`, pre-order
/// position 3) when both spent the same amount — a sort that fell back to
/// `HashMap` iteration order, or that compared depth, would flip these two
/// non-deterministically.
#[test]
fn ties_break_by_preorder_position_not_by_depth_or_branch() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let alpha_id = spawn_child(&mut app, &history, "Alpha", orchestrator_id, terminal_view_id);
        let grandchild_id =
            spawn_child(&mut app, &history, "Grandchild", alpha_id, terminal_view_id);
        let beta_id = spawn_child(&mut app, &history, "Beta", orchestrator_id, terminal_view_id);

        set_credits(&mut app, &history, orchestrator_id, 1.0);
        set_credits(&mut app, &history, alpha_id, 9.0);
        set_credits(&mut app, &history, grandchild_id, 4.0);
        set_credits(&mut app, &history, beta_id, 4.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("rollup should be Some");
            assert_eq!(rollup.total_credits, 18.0);
            let ordered: Vec<_> = rollup
                .per_agent
                .iter()
                .map(|entry| entry.conversation_id)
                .collect();
            assert_eq!(
                ordered,
                vec![alpha_id, grandchild_id, beta_id, orchestrator_id]
            );
            // And the documented ordering guarantee itself: credits descending.
            let credits: Vec<f32> = rollup
                .per_agent
                .iter()
                .map(|entry| entry.credits_spent)
                .collect();
            assert!(
                credits.windows(2).all(|w| w[0] >= w[1]),
                "per_agent must be sorted by credits descending, got {credits:?}"
            );
        });
    });
}

/// A descendant reachable from two parents contributes **one** summand and
/// **one** row.
///
/// `children_by_parent` permits the diamond (see
/// `orchestration_topology_tests::descendant_walk_dedups_a_child_reachable_from_two_parents`
/// for why), and this is the user-visible consequence of walking it naively:
/// the headline total exceeds the truth and the drill-down lists the same
/// agent twice — the mirror image of the defect the rollup headline was
/// introduced to fix, and just as wrong.
#[test]
fn a_descendant_reachable_from_two_parents_is_counted_once() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_a = spawn_child(
            &mut app,
            &history,
            "ChildA",
            orchestrator_id,
            terminal_view_id,
        );
        let child_b = spawn_child(
            &mut app,
            &history,
            "ChildB",
            orchestrator_id,
            terminal_view_id,
        );
        let shared = spawn_child(&mut app, &history, "Shared", child_a, terminal_view_id);
        // Re-parented onto `child_b`; the entry under `child_a` is not retracted.
        history.update(&mut app, |history, _| {
            history.set_parent_for_conversation(shared, child_b);
        });

        set_credits(&mut app, &history, orchestrator_id, 1.0);
        set_credits(&mut app, &history, child_a, 2.0);
        set_credits(&mut app, &history, child_b, 4.0);
        set_credits(&mut app, &history, shared, 8.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("orchestrator with spending descendants rolls up");

            assert_eq!(
                rollup.total_credits, 15.0,
                "1 + 2 + 4 + 8; the shared agent's 8 must not be added twice"
            );
            assert_eq!(
                rollup
                    .per_agent
                    .iter()
                    .filter(|entry| entry.conversation_id == shared)
                    .count(),
                1,
                "the shared agent must appear on exactly one drill-down row"
            );
            // The invariant the headline depends on: it equals the rows below it.
            let drill_down_sum: f32 = rollup
                .per_agent
                .iter()
                .map(|entry| entry.credits_spent)
                .sum();
            assert_eq!(rollup.total_credits, drill_down_sum);
        });
    });
}

/// The orchestrator's own spend is added exactly once even when the index
/// makes it reachable as one of its own descendants (a back-edge closing a
/// cycle). The walk seeds its visited set with the walk root precisely so the
/// prepended orchestrator entry above the descendant loop cannot be duplicated
/// by it.
#[test]
fn the_orchestrator_is_never_also_counted_as_its_own_descendant() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        let terminal_view_id = EntityId::new();
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let orchestrator_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_id = spawn_child(
            &mut app,
            &history,
            "Child",
            orchestrator_id,
            terminal_view_id,
        );
        // Back-edge: the orchestrator is indexed as a child of its own child.
        history.update(&mut app, |history, _| {
            history.set_parent_for_conversation(orchestrator_id, child_id);
        });

        set_credits(&mut app, &history, orchestrator_id, 3.0);
        set_credits(&mut app, &history, child_id, 5.0);

        history.read(&app, |history, _| {
            let rollup = compute_orchestration_rollup(orchestrator_id, history)
                .expect("orchestrator with a spending child rolls up");
            assert_eq!(
                rollup.total_credits, 8.0,
                "3 + 5, with no self-double-count"
            );
            assert_eq!(
                rollup
                    .per_agent
                    .iter()
                    .filter(|entry| entry.conversation_id == orchestrator_id)
                    .count(),
                1
            );
        });
    });
}
