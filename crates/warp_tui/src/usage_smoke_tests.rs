//! Chunk C of the local usage / acceptance smoke suite (see
//! `specs/usage-test-suite/SCOPE.md` §4.2).
//!
//! These are higher-level-than-unit **usage smoke tests**: each drives a real
//! TUI view/model subtree the way a user would reach it (fresh session,
//! transcript, a queued permission request, the completion / conversation /
//! slash-command menus) and asserts the rendered outcome. They are all
//! in-process render-snapshot / model-state assertions — no PTY, no shell, no
//! provider — so they are deterministic in this sandbox (SCOPE §1.2, all
//! `reliable-here`).
//!
//! Every test is named `usage_tui_*` so the runner's nextest filter
//! `test(/^usage_tui_/)` selects exactly this set.
//!
//! Note on the file name: SCOPE §6 named this file `usage_tests.rs`, but that
//! name is already taken by the footer context-usage-display unit tests
//! (`usage.rs` includes them via `#[path = "usage_tests.rs"] mod tests`). To
//! avoid the collision this module lives in `usage_smoke_tests.rs` and is wired
//! as a top-level `#[cfg(test)] mod usage_smoke_tests;` in `lib.rs`. The test
//! names — the only thing the runner's filter cares about — are unchanged.

use std::sync::Arc;

use parking_lot::FairMutex;
use warp::editor::CodeEditorModel;
use warp::tui_export::{
    AIAgentActionId, AcceptSlashCommandOrSavedPrompt, ActiveSession, Appearance,
    BlocklistAIPermissions, ModelEventDispatcher, Sessions, SlashCommandId, SlashCommandMixer,
    TerminalModel, TuiCompletionCandidate, register_tui_session_view_test_singletons,
};
use warpui::platform::WindowStyle;
use warpui::{AddWindowOptions, App, EntityId};
use warpui_core::elements::tui::{TuiBufferExt, TuiElement, TuiRect, TuiText};
use warpui_core::presenter::tui::TuiPresenter;
use warpui_core::{TuiView as _, ViewHandle, WindowInvalidation};

use crate::autoupdate::TuiAutoupdater;
use crate::completions_menu::TuiCompletionsMenuModel;
use crate::conversation_menu::TuiConversationMenuModel;
use crate::inline_menu::TuiInlineMenu;
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};
use crate::slash_commands::{TuiSlashCommandModel, TuiSlashCommandRow};
use crate::test_fixtures::{
    TestHostView, add_test_action_model, add_test_action_model_and_events,
    add_test_conversation_selection, settle,
};
use crate::transcript_view::TuiTranscriptView;
use crate::tui_permission_prompt::{TuiPermissionPrompt, render_permission_card};
use crate::zero_state::TuiZeroStateView;

/// `usage_tui_zero_state_render`: a fresh session's zero-state view renders the
/// product title. Builds the real `TuiZeroStateView` against the singleton
/// graph a live session provisions and asserts the "Warp Agent" header appears
/// in the rendered buffer (mirrors the outcome of
/// `terminal_session_view_tests::zero_state_renders_with_only_zero_height_bootstrap_blocks`,
/// but rendered directly so no shell bootstrap is required).
#[test]
fn usage_tui_zero_state_render() {
    App::test((), |mut app| async move {
        register_tui_session_view_test_singletons(&mut app);
        app.update(TuiAutoupdater::register);

        // A live `ActiveSession` is the only per-view dependency the zero state
        // needs; build it the same way `add_test_action_model_and_events` does.
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let (_events_tx, events_rx) = async_channel::unbounded();
        let dispatcher =
            app.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx));
        let active_session =
            app.add_model(|ctx| ActiveSession::new(sessions.clone(), dispatcher.clone(), ctx));

        let (window_id, view) = app.update(|ctx| {
            crate::zero_state_animation::ZeroStateAnimationConfig::register(ctx);
            let (window_id, _) = ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| TestHostView,
            );
            let view =
                ctx.add_tui_view(window_id, |ctx| TuiZeroStateView::new(active_session, ctx));
            (window_id, view)
        });

        let lines = present_view_lines(&mut app, window_id, &view, 120, 40);
        assert!(
            lines.iter().any(|line| line.contains("Warp Agent")),
            "fresh zero-state view should render the Warp Agent title;\ngot lines:\n{}",
            lines.join("\n")
        );
    });
}

/// `usage_tui_transcript_render`: seeding a mock terminal conversation and
/// rendering the transcript shows the command input and its output. Uses
/// `TerminalModel::mock` + `simulate_block` (no real shell) exactly as
/// `transcript_view_tests` does.
#[test]
fn usage_tui_transcript_render() {
    App::test((), |mut app| async move {
        // The block's command gutter is narrow, so keep the command short
        // enough not to wrap (as `transcript_view_tests` does with `echo 1`).
        let mut terminal_model = TerminalModel::mock(None, None);
        terminal_model.simulate_block("echo", "hello\r\n");
        let terminal_model = Arc::new(FairMutex::new(terminal_model));
        let model_for_view = terminal_model.clone();
        let (action_model, model_events) = add_test_action_model_and_events(&mut app);

        let (_window_id, transcript) = app.update(|ctx| {
            ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |ctx| {
                    TuiTranscriptView::new(
                        EntityId::new(),
                        model_for_view,
                        action_model,
                        &model_events,
                        ctx,
                    )
                },
            )
        });

        let mut presenter = TuiPresenter::new();
        let frame =
            app.update(|ctx| presenter.present(ctx, &transcript, TuiRect::new(0, 0, 80, 20)));
        let text = frame.buffer.to_lines().join("\n");

        assert!(
            text.contains("echo"),
            "transcript should render the command input:\n{text}"
        );
        assert!(
            text.contains("hello"),
            "transcript should render the command output:\n{text}"
        );
    });
}

/// `usage_tui_permission_prompt`: a queued blocking action surfaces a
/// permission prompt whose options render and default to "yes". Mirrors
/// `tui_permission_prompt_tests::permission_prompt_defaults_to_yes_and_renders_other`
/// but reads the selection through the public `highlighted_index` accessor.
#[test]
fn usage_tui_permission_prompt() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        if !app.read(|ctx| ctx.has_singleton_model::<BlocklistAIPermissions>()) {
            register_tui_session_view_test_singletons(&mut app);
        }
        let action_model = add_test_action_model(&mut app);

        let prompt = app.update(|ctx| {
            let (window_id, _) = ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| TestHostView,
            );
            ctx.add_typed_action_tui_view(window_id, move |ctx| {
                TuiPermissionPrompt::new(
                    action_model,
                    AIAgentActionId::from("usage-permission-action".to_owned()),
                    None,
                    ctx,
                )
            })
        });

        let lines = present_permission_card_lines(&mut app, &prompt);
        for option in ["(1) yes", "(2) no", "(3) Other"] {
            assert!(
                lines.iter().any(|line| line == option),
                "permission prompt should render option {option:?};\ngot lines:\n{}",
                lines.join("\n")
            );
        }
        app.read(|ctx| {
            assert_eq!(
                prompt.as_ref(ctx).highlighted_index(ctx),
                Some(0),
                "the prompt should default to the first option (yes)"
            );
        });
    });
}

/// `usage_tui_completions_menu`: opening the completions menu with rows renders
/// the entries, selects the first, and cycles the selection with the keyboard
/// navigation entry point. Mirrors `completions_menu_tests`.
#[test]
fn usage_tui_completions_menu() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));

            let opened = menu.update(ctx, |m, ctx| {
                m.show(
                    completion_rows(&[("checkout", "checkout"), ("cherry-pick", "cherry-pick")]),
                    4..7,
                    false,
                    ctx,
                )
            });
            assert!(opened, "menu with rows should open");
            assert!(menu.as_ref(ctx).is_open(ctx));
            assert_eq!(
                suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::Completions
            );

            let snapshot = menu.as_ref(ctx).snapshot(ctx).expect("menu is open");
            assert_eq!(snapshot.rows.len(), 2);
            assert_eq!(snapshot.selected_index, Some(0));
            assert_eq!(snapshot.rows[0].title, "checkout");

            menu.update(ctx, |m, ctx| m.select_next(ctx));
            assert_eq!(
                menu.as_ref(ctx).snapshot(ctx).unwrap().selected_index,
                Some(1),
                "keyboard navigation should advance the highlight"
            );
        });
    });
}

/// `usage_tui_conversation_menu`: the `/conversations` picker opens through the
/// real suggestions-mode transition and renders its "Conversations" header.
///
/// With no live conversation the list is empty, so this asserts the
/// deterministic slice of the open/render path (header, empty/loading status,
/// no highlight, navigation entry points are no-ops on an empty list).
/// Populated-row navigation reads a live conversation and is exercised by the
/// PTY harness instead (see the module note in `exchange_menu_tests.rs`).
#[test]
fn usage_tui_conversation_menu() {
    App::test((), |mut app| async move {
        register_tui_session_view_test_singletons(&mut app);

        let (_window_id, menu) = app.update(|ctx| {
            let (window_id, _) = ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| TestHostView,
            );
            let input_editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let conversation_selection = add_test_conversation_selection(ctx);
            let menu = ctx.add_model(|ctx| {
                TuiConversationMenuModel::new(
                    input_editor,
                    suggestions_mode,
                    conversation_selection,
                    window_id,
                    ctx,
                )
            });
            (window_id, menu)
        });

        menu.read(&app, |m, ctx| {
            assert!(!m.is_open(ctx), "menu starts closed")
        });

        menu.update(&mut app, |m, ctx| m.open(ctx));
        settle().await;

        menu.read(&app, |m, ctx| {
            assert!(m.is_open(ctx), "picker should open");
            let snapshot = m.snapshot(ctx).expect("open menu has a snapshot");
            assert_eq!(
                snapshot
                    .header
                    .as_ref()
                    .and_then(|header| header.title.as_deref()),
                Some("Conversations"),
                "picker should render the Conversations header"
            );
            assert!(
                snapshot.status.is_some(),
                "an empty picker should show a loading/empty status"
            );
            assert_eq!(
                snapshot.selected_index, None,
                "an empty list has no highlighted row"
            );
        });

        // Navigation entry points are wired and remain no-ops on an empty list.
        menu.update(&mut app, |m, ctx| m.select_next(ctx));
        menu.read(&app, |m, ctx| {
            assert_eq!(m.snapshot(ctx).unwrap().selected_index, None);
        });
    });
}

/// `usage_tui_slash_command_palette`: the slash-command menu renders the
/// supported non-cloud commands. Mirrors
/// `slash_commands_tests::slash_command_menu_renders_view_logs_row`, asserting a
/// representative set of the portable commands.
#[test]
fn usage_tui_slash_command_palette() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| Appearance::mock());
            let input_editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            suggestions_mode.update(ctx, |mode, ctx| {
                mode.set_mode(TuiInputSuggestionsMode::SlashCommands, ctx);
            });
            let mixer = ctx.add_model(|_| SlashCommandMixer::new());
            let conversation_selection = add_test_conversation_selection(ctx);
            let model = ctx.add_model(|_| {
                TuiSlashCommandModel::new_for_test(
                    input_editor,
                    suggestions_mode,
                    mixer,
                    conversation_selection,
                    vec![
                        slash_row("/view-logs", "Bundle your TUI logs into a zip archive"),
                        slash_row("/export-to-file", "Export the conversation to a file"),
                        slash_row("/compact", "Summarize and compact the conversation"),
                    ],
                    0,
                )
            });
            let menu = TuiInlineMenu::new(model.clone());
            let element = menu.render(ctx).expect("slash-command menu should render");

            let mut presenter = TuiPresenter::new();
            let lines = presenter
                .present_element(element, TuiRect::new(0, 0, 80, 20), ctx)
                .buffer
                .to_lines();
            for command in ["/view-logs", "/export-to-file", "/compact"] {
                assert!(
                    lines.iter().any(|line| line.contains(command)),
                    "slash-command menu should render {command:?};\ngot lines:\n{}",
                    lines.join("\n")
                );
            }
        });
    });
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn completion_rows(pairs: &[(&str, &str)]) -> Vec<TuiCompletionCandidate> {
    pairs
        .iter()
        .map(|(display, replacement)| TuiCompletionCandidate {
            display: (*display).to_owned(),
            replacement: (*replacement).to_owned(),
            description: None,
            // These fixtures are command completions, not paths, so no
            // directory-suppression of the trailing space applies.
            is_directory: false,
        })
        .collect()
}

fn slash_row(title: &str, description: &str) -> TuiSlashCommandRow {
    TuiSlashCommandRow {
        title: title.to_owned(),
        description: Some(description.to_owned()),
        action: AcceptSlashCommandOrSavedPrompt::SlashCommand {
            id: SlashCommandId::new(),
        },
    }
}

/// Renders a `TuiView` handle into trimmed, non-empty text lines.
fn present_view_lines<V: warpui_core::TuiView>(
    app: &mut App,
    window_id: warpui_core::WindowId,
    view: &ViewHandle<V>,
    width: u16,
    height: u16,
) -> Vec<String> {
    let mut presenter = TuiPresenter::new();
    let frame = app.update(|ctx| {
        let mut invalidation = WindowInvalidation::default();
        invalidation.updated.insert(view.id());
        invalidation
            .updated
            .extend(view.as_ref(ctx).child_view_ids(ctx));
        presenter.invalidate(&invalidation, ctx, window_id);
        presenter.present(ctx, view, TuiRect::new(0, 0, width, height))
    });
    frame.buffer.to_lines()
}

/// Renders the permission card for `prompt` into trimmed, non-empty lines,
/// invalidating the prompt and its child selector so the option list paints.
fn present_permission_card_lines(
    app: &mut App,
    prompt: &ViewHandle<TuiPermissionPrompt>,
) -> Vec<String> {
    let mut presenter = TuiPresenter::new();
    app.update(|ctx| {
        let mut invalidation = WindowInvalidation::default();
        invalidation.updated.insert(prompt.id());
        invalidation
            .updated
            .extend(prompt.as_ref(ctx).child_view_ids(ctx));
        presenter.invalidate(&invalidation, ctx, prompt.window_id(ctx));
        presenter
            .present_element(
                render_permission_card(
                    prompt,
                    "Permission",
                    Some(TuiText::new("details").finish()),
                    None,
                    ctx,
                ),
                TuiRect::new(0, 0, 80, 12),
                ctx,
            )
            .buffer
            .to_lines()
            .into_iter()
            .map(|line| line.trim().to_owned())
            .filter(|line| !line.is_empty())
            .collect()
    })
}
