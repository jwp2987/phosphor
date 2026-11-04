use super::{
    CollapsibleElementState, CollapsibleExpansionState, EmptyEnterHint,
    default_collapsible_state_for_orchestration_action,
    default_collapsible_state_for_orchestration_message,
};
use crate::ai::agent::{AIAgentActionId, AIAgentActionType};
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

    if let AutonomySettingSpeedbump::ShouldShowForAskUserQuestion { shown: latch, .. } = &speedbump
    {
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

// ── Model-named file paths and the OS default handler (#675) ──

/// A raster image detected in AI output opens in the system image viewer, as before.
#[test]
#[cfg(feature = "local_fs")]
fn detected_raster_image_path_opens_in_system_viewer() {
    use crate::util::openable_file_type::FileTarget;

    for path in [
        "/tmp/photo.png",
        "/tmp/photo.JPG",
        "/tmp/anim.gif",
        "/tmp/a.webp",
    ] {
        assert_eq!(
            super::detected_file_path_target_override(std::path::Path::new(path)),
            Some(FileTarget::SystemGeneric),
            "{path}"
        );
    }
}

/// An SVG named by the model must NOT be forced to `SystemGeneric`: its default handler is
/// normally a browser, which runs any `<script>` the file embeds. With no override it goes
/// through `resolve_file_target`, which never picks the OS handler for SVG (see
/// `openable_file_type::tests::svg_never_resolves_to_os_handler`).
#[test]
#[cfg(feature = "local_fs")]
fn detected_svg_path_is_not_handed_to_os_handler() {
    for path in [
        "/tmp/model-named.svg",
        "/tmp/model-named.SVG",
        "/tmp/x.png/evil.Svg",
    ] {
        assert_eq!(
            super::detected_file_path_target_override(std::path::Path::new(path)),
            None,
            "{path}"
        );
    }
}

/// Non-image paths keep their existing behaviour: no override.
#[test]
#[cfg(feature = "local_fs")]
fn detected_non_image_path_has_no_override() {
    for path in ["/tmp/main.rs", "/tmp/README.md", "/tmp/noext"] {
        assert_eq!(
            super::detected_file_path_target_override(std::path::Path::new(path)),
            None,
            "{path}"
        );
    }
}

// ── Model-named launchable paths are revealed, never opened (#681) ──

/// The issue's case: a model names `Evil.app` / `setup.pkg` / `setup.exe` in an AI block. There
/// is no override, so `TerminalView::open_file_path` resolves it with `resolve_file_target` --
/// which must now pick Reveal, not `SystemGeneric`, under every editor choice.
#[test]
#[cfg(feature = "local_fs")]
fn detected_launchable_path_resolves_to_reveal() {
    use crate::util::file::external_editor::{Editor, settings::EditorChoice};
    use crate::util::openable_file_type::{
        EditorLayout, FileTarget, resolve_file_target_with_editor_choice,
    };

    for path in [
        "/tmp/Evil.app",
        "/tmp/setup.pkg",
        "/tmp/image.dmg",
        "/tmp/setup.exe",
        "/tmp/setup.msi",
        "/tmp/report.xlsx",
    ] {
        let path = std::path::Path::new(path);
        assert_eq!(super::detected_file_path_target_override(path), None);
        for editor_choice in [
            EditorChoice::Zap,
            EditorChoice::SystemDefault,
            EditorChoice::ExternalEditor(Editor::VSCode),
        ] {
            assert_eq!(
                resolve_file_target_with_editor_choice(
                    path,
                    editor_choice,
                    false, /* prefer_markdown_viewer */
                    EditorLayout::SplitPane,
                    None,
                ),
                FileTarget::RevealInFileManager,
                "{path:?} under {editor_choice:?}"
            );
        }
    }
}

/// A model-named `.png` that is actually an executable (execute bit + ELF header). The hover-
/// time override stays extension-only -- no filesystem access on hover (#681 review) -- and the
/// path is caught at click time: it is launchable, so the workspace sink and
/// `AppContext::open_file_path` reveal it (`workspace::view_test::
/// test_open_file_with_target_reveals_launchable_paths` covers the sink for `SystemGeneric`).
#[test]
#[cfg(all(feature = "local_fs", unix))]
fn detected_raster_image_that_is_an_executable_is_caught_at_click_time() {
    use crate::util::openable_file_type::{
        FileTarget, guard_system_handler_target, is_launchable_path,
    };
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let disguised = dir.path().join("photo.png");
    std::fs::write(&disguised, b"\x7fELF\x02\x01\x01").unwrap();
    std::fs::set_permissions(&disguised, std::fs::Permissions::from_mode(0o755)).unwrap();

    let hover_time = super::detected_file_path_target_override(&disguised);
    assert_eq!(hover_time, Some(FileTarget::SystemGeneric));
    assert!(is_launchable_path(&disguised));
    assert_eq!(
        guard_system_handler_target(&disguised, hover_time.unwrap()),
        FileTarget::RevealInFileManager
    );
}

// ── Tooltip dismissal only repaints when something changed (#677) ──

/// `dismiss_ai_tooltips` gates its repaint on this return value, so it must report `true` only
/// when a tooltip was actually open.
#[test]
fn secret_tooltip_dismiss_reports_whether_one_was_open() {
    use super::TextLocation;
    use super::secret_redaction::SecretRedactionState;
    use warpui::elements::SecretRange;

    let mut state = SecretRedactionState::default();
    assert!(!state.dismiss_tooltip(), "nothing open");

    let location = TextLocation::Output {
        section_index: 0,
        line_index: 0,
    };
    let range = SecretRange {
        char_range: 0..4,
        byte_range: 0..4,
    };
    state.show_secret_tooltip(&location, &range);
    assert!(state.dismiss_tooltip(), "an open tooltip was closed");
    assert!(state.open_tooltip_location().is_none());
    assert!(!state.dismiss_tooltip(), "second dismiss is a no-op");
}

/// A code-snippet "open" button that was never hovered needs no repaint on focus change.
#[test]
fn unhovered_code_snippet_button_reset_reports_no_change() {
    let handles = crate::ai::blocklist::code_block::CodeSnippetButtonHandles::default();
    assert!(!handles.reset_hover_state_on_focus_change());
}

#[test]
fn empty_enter_hint_follows_focus_and_pending_card() {
    let card: AIAgentActionId = "card".to_string().into();
    let other: AIAgentActionId = "other".to_string().into();
    let mut hint = EmptyEnterHint::default();

    // Nothing is shown until the empty-prompt Enter focuses the card.
    assert_eq!(hint.text_for(&card, Some(&card)), None);

    hint.show(card.clone(), "run");
    assert_eq!(
        hint.text_for(&card, Some(&card)).as_deref(),
        Some("Press Enter again to run")
    );
    // Only the focused card shows it, and only while it is still the pending action.
    assert_eq!(hint.text_for(&other, Some(&card)), None);
    assert_eq!(hint.text_for(&card, Some(&other)), None);
    // The card resolved (approved or rejected): no pending action, no hint.
    assert_eq!(hint.text_for(&card, None), None);

    // Focus leaving the card turns it off for good, and says whether a repaint is needed.
    assert!(hint.clear());
    assert_eq!(hint.text_for(&card, Some(&card)), None);
    assert!(!hint.clear());

    hint.show(card.clone(), "apply");
    assert_eq!(
        hint.text_for(&card, Some(&card)).as_deref(),
        Some("Press Enter again to apply")
    );
}

// ---------------------------------------------------------------- #1267 code-block editor recycling
//
// An AIBlock cannot be built in a unit test (it needs a terminal model, controllers, the history and
// action models and a live conversation; no test in this file builds one). So the tests drive the
// narrowest REAL functions the block delegates to: `new_snippet_editor` + `configure_snippet_editor`
// (what `AIBlock::build_code_editor_view` builds, minus its event subscription to the block),
// `recycle_embedded_editors`, `restore_embedded_editors` and `rebuild_recycled_for_stream` (the bodies
// of `recycle_code_editors`, `restore_code_editors` and the recycled arm of
// `handle_code_section_stream_update`), the distance plan `ai_code_recycle_plan` that
// `TerminalView::recycle_ai_code_editors` executes, and the kill-switch parser. The `is_finished`
// guard is a single condition inside `AIBlock::recycle_code_editors` and is not reachable here.

mod code_editor_recycling {
    use warp_editor::render::element::VerticalExpansionBehavior;
    use warpui::platform::WindowStyle;
    use warpui::{App, EntityId, TypedActionView, ViewHandle, WindowId};

    use super::super::{
        EmbeddedCodeEditorView, configure_snippet_editor, new_snippet_editor,
        rebuild_recycled_for_stream, recycle_embedded_editors, recycling_flag_enabled,
        restore_embedded_editors,
    };
    use crate::code::editor::view::view_tests::init_code_editor_test_singletons;
    use crate::code::editor::view::{
        CodeEditorRenderOptions, CodeEditorView, CodeEditorViewAction,
    };
    use crate::terminal::view::{
        AICodeRecycleAction, ai_code_recycle_action, ai_code_recycle_plan,
    };

    fn init(app: &mut App) -> WindowId {
        init_code_editor_test_singletons(app);
        let (window, _root) = app.add_window(WindowStyle::NotStealFocus, |ctx| {
            CodeEditorView::new(
                None,
                None,
                CodeEditorRenderOptions::new(VerticalExpansionBehavior::InfiniteHeight),
                ctx,
            )
        });
        window
    }

    /// The real snippet-editor construction and configuration the block uses.
    fn snippet(
        app: &mut App,
        window: WindowId,
        code: &str,
        line: Option<usize>,
    ) -> ViewHandle<CodeEditorView> {
        let view = app.add_typed_action_view(window, |ctx| new_snippet_editor(None, ctx));
        let code = code.to_string();
        view.update(app, move |view, ctx| {
            configure_snippet_editor(view, &code, &None, line, ctx);
        });
        view
    }

    fn text_of(app: &App, view: &ViewHandle<CodeEditorView>) -> String {
        view.read(app, |view, ctx| view.text(ctx).into_string())
    }

    const CODE_A: &str = "fn main() {\n    println!(\"a\");\n}\n";
    const CODE_B: &str = "let x = 1;\r\nlet y = 2;\r\n";

    #[test]
    fn far_blocks_recycle_near_blocks_rebuild_with_hysteresis() {
        assert_eq!(ai_code_recycle_action(0), AICodeRecycleAction::Rebuild);
        assert_eq!(ai_code_recycle_action(2), AICodeRecycleAction::Rebuild);
        assert_eq!(ai_code_recycle_action(3), AICodeRecycleAction::Keep);
        assert_eq!(ai_code_recycle_action(6), AICodeRecycleAction::Keep);
        assert_eq!(ai_code_recycle_action(7), AICodeRecycleAction::Recycle);
    }

    #[test]
    fn the_plan_measures_rich_content_distance_from_the_nearest_visible_entry() {
        let ids: Vec<EntityId> = (0..20).map(EntityId::from_usize).collect();
        // Every entry is an AI block except 5 (another kind of rich content).
        let rich: Vec<(EntityId, bool)> = ids.iter().map(|id| (*id, *id != ids[5])).collect();

        // Nothing visible: nothing is planned, nothing recycled.
        assert!(ai_code_recycle_plan(&rich, &[]).is_empty());
        // A visible id that is not rich content counts as nothing visible.
        assert!(ai_code_recycle_plan(&rich, &[EntityId::from_usize(999)]).is_empty());

        let plan = ai_code_recycle_plan(&rich, &[ids[0]]);
        let action = |plan: &[(usize, AICodeRecycleAction)], position: usize| {
            plan.iter().find(|(p, _)| *p == position).map(|(_, a)| *a)
        };
        assert_eq!(action(&plan, 2), Some(AICodeRecycleAction::Rebuild));
        assert_eq!(action(&plan, 3), Some(AICodeRecycleAction::Keep));
        assert_eq!(action(&plan, 6), Some(AICodeRecycleAction::Keep));
        assert_eq!(action(&plan, 7), Some(AICodeRecycleAction::Recycle));
        assert_eq!(
            action(&plan, 5),
            None,
            "non-AI rich content is never planned"
        );
        assert_eq!(plan.len(), 19);

        // A block between two visible entries takes the nearer one: 10 is 4 from 6 and 4 from 14.
        let plan = ai_code_recycle_plan(&rich, &[ids[6], ids[14]]);
        assert_eq!(action(&plan, 10), Some(AICodeRecycleAction::Keep));
        assert_eq!(action(&plan, 12), Some(AICodeRecycleAction::Rebuild));
        assert_eq!(action(&plan, 19), Some(AICodeRecycleAction::Keep));
        // 0 is 6 from 6: kept, not recycled.
        assert_eq!(action(&plan, 0), Some(AICodeRecycleAction::Keep));
        let plan = ai_code_recycle_plan(&rich, &[ids[14]]);
        assert_eq!(action(&plan, 0), Some(AICodeRecycleAction::Recycle));
    }

    #[test]
    fn the_kill_switch_turns_recycling_off_only_on_an_explicit_off() {
        assert!(recycling_flag_enabled(None));
        assert!(recycling_flag_enabled(Some("1")));
        assert!(!recycling_flag_enabled(Some("0")));
        assert!(!recycling_flag_enabled(Some(" Off ")));
        assert!(!recycling_flag_enabled(Some("false")));
    }

    #[test]
    fn a_recycled_editor_is_rebuilt_from_the_original_snippet_text() {
        App::test((), |mut app| async move {
            let window = init(&mut app);
            let a = snippet(&mut app, window, CODE_A, Some(41));
            let b = snippet(&mut app, window, CODE_B, None);
            let mut views = vec![
                EmbeddedCodeEditorView::new(a, None, CODE_A.len()),
                EmbeddedCodeEditorView::new(b, None, CODE_B.len()),
            ];
            let codes = vec![CODE_A.to_string(), CODE_B.to_string()];

            let dropped = app.read(|ctx| recycle_embedded_editors(&mut views, &codes, ctx));
            assert_eq!(dropped, 2);
            assert!(views.iter().all(|v| v.view.is_none()));
            let kept_a = views[0].recycled.as_ref().unwrap();
            assert_eq!(
                (kept_a.code.as_str(), kept_a.starting_line_number),
                (CODE_A, Some(41))
            );
            // The kept text is the section's own, CRLF included, not a re-serialised buffer.
            assert_eq!(views[1].recycled.as_ref().unwrap().code, CODE_B);
            // Nothing is left to drop a second time.
            assert_eq!(
                app.read(|ctx| recycle_embedded_editors(&mut views, &codes, ctx)),
                0
            );

            let mut built = Vec::new();
            let rebuilt = restore_embedded_editors(&mut views, |code, _language, line| {
                built.push((code.to_string(), line));
                snippet(&mut app, window, code, line)
            });
            assert_eq!(rebuilt, 2);
            assert_eq!(
                built,
                vec![(CODE_A.to_string(), Some(41)), (CODE_B.to_string(), None)]
            );
            assert!(
                views
                    .iter()
                    .all(|v| v.view.is_some() && v.recycled.is_none())
            );
            let rebuilt_a = views[0].view.clone().unwrap();
            assert_eq!(text_of(&app, &rebuilt_a), CODE_A);
            assert_eq!(
                rebuilt_a.read(&app, |view, _| view.starting_line_number()),
                Some(41)
            );
        });
    }

    #[test]
    fn an_editor_without_its_section_text_is_kept() {
        App::test((), |mut app| async move {
            let window = init(&mut app);
            let a = snippet(&mut app, window, CODE_A, None);
            let mut views = vec![EmbeddedCodeEditorView::new(a, None, CODE_A.len())];
            assert_eq!(
                app.read(|ctx| recycle_embedded_editors(&mut views, &[], ctx)),
                0
            );
            assert!(views[0].view.is_some());
        });
    }

    #[test]
    fn a_stream_update_after_a_recycle_rebuilds_with_the_new_text() {
        App::test((), |mut app| async move {
            let window = init(&mut app);
            let a = snippet(&mut app, window, CODE_A, None);
            let mut views = vec![EmbeddedCodeEditorView::new(a, None, CODE_A.len())];
            let codes = vec![CODE_A.to_string()];
            assert_eq!(
                app.read(|ctx| recycle_embedded_editors(&mut views, &codes, ctx)),
                1
            );

            let longer = format!("{CODE_A}fn more() {{}}\n");
            let rebuilt = rebuild_recycled_for_stream(
                &mut views[0],
                &longer,
                &None,
                Some(3),
                |code, _, line| snippet(&mut app, window, code, line),
            );
            assert!(rebuilt);
            assert!(views[0].recycled.is_none());
            assert_eq!(views[0].length, longer.len());
            let view = views[0].view.clone().unwrap();
            assert_eq!(text_of(&app, &view), longer);
            assert_eq!(
                view.read(&app, |view, _| view.starting_line_number()),
                Some(3)
            );

            // An editor that is not recycled is left to the in-place update.
            assert!(!rebuild_recycled_for_stream(
                &mut views[0],
                "x",
                &None,
                None,
                |_, _, _| { panic!("a live editor must not be rebuilt") }
            ));
        });
    }

    #[test]
    fn a_selected_focused_or_searching_editor_is_never_dropped() {
        App::test((), |mut app| async move {
            let window = init(&mut app);
            let selected = snippet(&mut app, window, CODE_A, None);
            let focused = snippet(&mut app, window, CODE_B, None);
            let searching = snippet(&mut app, window, CODE_A, None);
            let going_to_line = snippet(&mut app, window, CODE_A, None);
            let plain = snippet(&mut app, window, CODE_A, None);
            selected.update(&mut app, |view, ctx| {
                view.handle_action(&CodeEditorViewAction::SelectAll, ctx);
            });
            // Opening the find bar or the go-to-line dialog focuses that dialog, and a window has
            // one focused view, so the editor that keeps its focus is focused last.
            searching.update(&mut app, |view, ctx| {
                view.handle_action(&CodeEditorViewAction::ShowFindBar, ctx);
            });
            going_to_line.update(&mut app, |view, ctx| {
                view.handle_action(&CodeEditorViewAction::ShowGoToLine, ctx);
            });
            focused.update(&mut app, |view, ctx| view.focus(ctx));
            assert!(selected.read(&app, |view, ctx| view.holds_user_state(ctx)));
            assert!(focused.read(&app, |view, ctx| view.holds_user_state(ctx)));
            assert!(
                searching.read(&app, |view, ctx| view.holds_user_state(ctx)),
                "an open find bar is user state"
            );
            assert!(
                going_to_line.read(&app, |view, ctx| view.holds_user_state(ctx)),
                "an open go-to-line dialog is user state"
            );
            assert!(!plain.read(&app, |view, ctx| view.holds_user_state(ctx)));

            let mut views = vec![
                EmbeddedCodeEditorView::new(selected, None, CODE_A.len()),
                EmbeddedCodeEditorView::new(focused, None, CODE_B.len()),
                EmbeddedCodeEditorView::new(searching, None, CODE_A.len()),
                EmbeddedCodeEditorView::new(going_to_line, None, CODE_A.len()),
                EmbeddedCodeEditorView::new(plain, None, CODE_A.len()),
            ];
            let codes = vec![
                CODE_A.to_string(),
                CODE_B.to_string(),
                CODE_A.to_string(),
                CODE_A.to_string(),
                CODE_A.to_string(),
            ];
            let dropped = app.read(|ctx| recycle_embedded_editors(&mut views, &codes, ctx));
            assert_eq!(dropped, 1, "only the editor without user state goes");
            assert!(views[..4].iter().all(|v| v.view.is_some()));
            assert!(views[4].view.is_none());
        });
    }
}
