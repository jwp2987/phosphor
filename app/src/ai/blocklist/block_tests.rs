use super::{
    CollapsibleElementState, CollapsibleExpansionState,
    default_collapsible_state_for_orchestration_action,
    default_collapsible_state_for_orchestration_message,
};
use crate::ai::agent::AIAgentActionType;
use crate::settings::{AISettings, OrchestrationMessageDisplayMode};
use crate::test_util::settings::initialize_settings_for_tests;
use settings::Setting;
use warpui::{App, SingletonEntity};

#[test]
fn reasoning_auto_collapses_when_user_has_not_manually_toggled() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::default();
        app.update(|ctx| {
            state.finish_reasoning(ctx);
        });

        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Collapsed
        ));
    });
}

#[test]
fn always_show_thinking_stays_expanded_after_finish() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .thinking_display_mode
                .set_value(crate::settings::ThinkingDisplayMode::AlwaysShow, ctx)
                .unwrap();
        });

        let mut state = CollapsibleElementState::default();
        app.update(|ctx| {
            state.finish_reasoning(ctx);
        });

        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Expanded {
                is_finished: true,
                scroll_pinned_to_bottom: false
            }
        ));
    });
}

#[test]
fn manual_collapse_while_streaming_stays_collapsed_after_finish() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::default();

        state.toggle_expansion();
        app.update(|ctx| {
            state.finish_reasoning(ctx);
        });

        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Collapsed
        ));
    });
}

#[test]
fn manual_reexpand_while_streaming_stays_expanded_after_finish() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::default();

        state.toggle_expansion();
        state.toggle_expansion();
        app.update(|ctx| {
            state.finish_reasoning(ctx);
        });

        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Expanded {
                is_finished: true,
                scroll_pinned_to_bottom: false
            }
        ));
    });
}

// Ported from the pinned Warp oracle `02b53fcd8`
// (`app/src/ai/blocklist/block_tests.rs`).

#[test]
fn collapsed_initializer_starts_collapsed() {
    let state = CollapsibleElementState::collapsed();

    assert!(matches!(
        state.expansion_state,
        CollapsibleExpansionState::Collapsed
    ));
}

#[test]
fn orchestration_send_message_starts_collapsed() {
    let state = default_collapsible_state_for_orchestration_action(
        &AIAgentActionType::SendMessageToAgent {
            addresses: vec!["child-agent".to_string()],
            subject: "Status".to_string(),
            message: "Body".to_string(),
        },
        OrchestrationMessageDisplayMode::AlwaysCollapse,
    )
    .expect("send-message actions should get a collapsible state");

    assert!(matches!(
        state.expansion_state,
        CollapsibleExpansionState::Collapsed
    ));
}

#[test]
fn non_orchestration_actions_do_not_get_collapsible_state_defaults() {
    assert!(
        default_collapsible_state_for_orchestration_action(
            &AIAgentActionType::OpenCodeReview,
            OrchestrationMessageDisplayMode::AlwaysCollapse,
        )
        .is_none()
    );
}

#[test]
fn orchestration_show_and_collapse_starts_sent_messages_expanded() {
    let state = default_collapsible_state_for_orchestration_action(
        &AIAgentActionType::SendMessageToAgent {
            addresses: vec!["child-agent".to_string()],
            subject: "Status".to_string(),
            message: "Body".to_string(),
        },
        OrchestrationMessageDisplayMode::ShowAndCollapse,
    )
    .expect("send-message actions should get a collapsible state");

    assert!(matches!(
        state.expansion_state,
        CollapsibleExpansionState::Expanded {
            is_finished: false,
            scroll_pinned_to_bottom: true
        }
    ));
}

#[test]
fn orchestration_always_show_starts_sent_messages_expanded() {
    let state = default_collapsible_state_for_orchestration_action(
        &AIAgentActionType::SendMessageToAgent {
            addresses: vec!["child-agent".to_string()],
            subject: "Status".to_string(),
            message: "Body".to_string(),
        },
        OrchestrationMessageDisplayMode::AlwaysShow,
    )
    .expect("send-message actions should get a collapsible state");

    assert!(matches!(
        state.expansion_state,
        CollapsibleExpansionState::Expanded {
            is_finished: false,
            scroll_pinned_to_bottom: true
        }
    ));
}

#[test]
fn orchestration_show_and_collapse_collapses_after_finish() {
    let mut state = default_collapsible_state_for_orchestration_message(
        OrchestrationMessageDisplayMode::ShowAndCollapse,
    );

    state.finish_orchestration_message(OrchestrationMessageDisplayMode::ShowAndCollapse);

    assert!(matches!(
        state.expansion_state,
        CollapsibleExpansionState::Collapsed
    ));
}

#[test]
fn orchestration_always_show_stays_expanded_after_finish() {
    let mut state = default_collapsible_state_for_orchestration_message(
        OrchestrationMessageDisplayMode::AlwaysShow,
    );

    state.finish_orchestration_message(OrchestrationMessageDisplayMode::AlwaysShow);

    assert!(matches!(
        state.expansion_state,
        CollapsibleExpansionState::Expanded {
            is_finished: true,
            scroll_pinned_to_bottom: false
        }
    ));
}

#[test]
fn orchestration_received_messages_follow_initial_message_display_mode() {
    let show_and_collapse = default_collapsible_state_for_orchestration_message(
        OrchestrationMessageDisplayMode::ShowAndCollapse,
    );
    assert!(matches!(
        show_and_collapse.expansion_state,
        CollapsibleExpansionState::Expanded {
            is_finished: false,
            scroll_pinned_to_bottom: true
        }
    ));
    let collapsed = default_collapsible_state_for_orchestration_message(
        OrchestrationMessageDisplayMode::AlwaysCollapse,
    );
    assert!(matches!(
        collapsed.expansion_state,
        CollapsibleExpansionState::Collapsed
    ));
    let expanded = default_collapsible_state_for_orchestration_message(
        OrchestrationMessageDisplayMode::AlwaysShow,
    );

    assert!(matches!(
        expanded.expansion_state,
        CollapsibleExpansionState::Expanded {
            is_finished: false,
            scroll_pinned_to_bottom: true
        }
    ));
}

#[cfg(feature = "local_fs")]
#[test]
fn open_code_action_routes_links_to_configured_editor_and_non_links_to_warp() {
    use ai::skills::SkillReference;
    use std::path::PathBuf;
    use warp_util::path::LineAndColumnArg;

    use super::{AIBlockEvent, open_code_action_event};
    use crate::code::editor_management::CodeSource;

    let linked_source = CodeSource::Link {
        path: PathBuf::from("/workspace/project/src/main.rs"),
        range_start: Some(LineAndColumnArg {
            line_num: 42,
            column_num: Some(7),
        }),
        range_end: None,
    };

    assert!(matches!(
        open_code_action_event(
            &linked_source,
            crate::util::file::external_editor::settings::EditorLayout::SplitPane,
        ),
        AIBlockEvent::OpenDetectedFilePath {
            absolute_path,
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 42,
                column_num: Some(7),
            }),
            target_override: None,
        } if absolute_path.as_path() == std::path::Path::new("/workspace/project/src/main.rs")
    ));

    // Adaptation: this fork's `CodeSource::Skill` carries a plain `PathBuf`
    // `path` rather than the oracle's `location: LocalOrRemotePath` (that field
    // stays local-editor-pane-only — see `editor_management.rs`). As of #299,
    // `SkillReference::Path` does wrap a `LocalOrRemotePath`, matching the pin.
    // The routing behaviour under test is unchanged.
    let skill_source = CodeSource::Skill {
        reference: SkillReference::Path(warp_util::local_or_remote_path::LocalOrRemotePath::Local(
            PathBuf::from("/workspace/project/.warp/skills/example/SKILL.md"),
        )),
        path: PathBuf::from("/workspace/project/.warp/skills/example/SKILL.md"),
        origin: crate::ai::skills::SkillOpenOrigin::ReadSkill,
    };

    assert!(matches!(
        open_code_action_event(
            &skill_source,
            crate::util::file::external_editor::settings::EditorLayout::NewTab,
        ),
        AIBlockEvent::OpenCodeInWarp {
            source,
            layout: crate::util::file::external_editor::settings::EditorLayout::NewTab,
        } if source == skill_source
    ));
}

// ── Ask-User-Question autonomy speedbump (#11) ──
//
// The `should_show_agent_mode_ask_user_question_speedbump` setting existed with
// zero consumers because `AutonomySettingSpeedbump` had no matching variant.
// These tests pin the variant's contract: it is keyed by action id, and its
// `shown` latch is what gates consuming the one-shot setting.

#[test]
fn ask_user_question_speedbump_is_keyed_by_action_id() {
    use crate::ai::agent::AIAgentActionId;
    use crate::ai::blocklist::block::AutonomySettingSpeedbump;
    use parking_lot::Mutex;
    use std::sync::Arc;

    let action_id = AIAgentActionId::from("action-1".to_string());
    let other_id = AIAgentActionId::from("action-2".to_string());
    let speedbump = AutonomySettingSpeedbump::ShouldShowForAskUserQuestion {
        action_id: action_id.clone(),
        shown: Arc::new(Mutex::new(false)),
    };

    assert!(matches!(
        &speedbump,
        AutonomySettingSpeedbump::ShouldShowForAskUserQuestion { action_id: id, .. }
            if *id == action_id
    ));
    assert!(!matches!(
        &speedbump,
        AutonomySettingSpeedbump::ShouldShowForAskUserQuestion { action_id: id, .. }
            if *id == other_id
    ));
}

#[test]
fn ask_user_question_speedbump_shown_latch_starts_unset() {
    use crate::ai::agent::AIAgentActionId;
    use crate::ai::blocklist::block::AutonomySettingSpeedbump;
    use parking_lot::Mutex;
    use std::sync::Arc;

    let shown = Arc::new(Mutex::new(false));
    let speedbump = AutonomySettingSpeedbump::ShouldShowForAskUserQuestion {
        action_id: AIAgentActionId::from("action-1".to_string()),
        shown: shown.clone(),
    };

    // The one-shot setting is only consumed once the footer actually renders,
    // so a seeded-but-never-shown speedbump must leave the latch clear.
    assert!(!*shown.lock());

    if let AutonomySettingSpeedbump::ShouldShowForAskUserQuestion { shown: latch, .. } = &speedbump {
        *latch.lock() = true;
    }
    assert!(*shown.lock());
}

/// Ported from the pin (`42effe840:app/src/ai/blocklist/block_tests.rs::
/// received_message_collapsible_id_prefixes_row_ids`) verbatim --
/// `received_message_collapsible_id` (`block.rs:837`) is byte-identical to the
/// pin's (`block.rs:877`), so no adaptation was needed.
///
/// The prefix is what keeps a received orchestration message's collapsible row
/// id from colliding with any other `MessageId` in the same block: the
/// transcript keys `collapsible_states` by `MessageId`, and the raw
/// `message_id` of the received message is also used as an id elsewhere
/// (`view_impl/orchestration.rs:390`). Without the namespace, expanding one
/// would toggle the other.
#[test]
fn received_message_collapsible_id_prefixes_row_ids() {
    let first = super::received_message_collapsible_id("message-1");
    let second = super::received_message_collapsible_id("message-2");

    assert_eq!(&*first, "received-message:message-1");
    assert_eq!(&*second, "received-message:message-2");
    assert_ne!(first, second);
}
