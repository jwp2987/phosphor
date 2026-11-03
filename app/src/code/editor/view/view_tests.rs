use warp_core::ui::appearance::Appearance;
use warp_editor::render::element::VerticalExpansionBehavior;
use warpui::{
    elements::{new_scrollable::ScrollableAppearance, ScrollbarWidth},
    platform::WindowStyle,
    App, TypedActionView, ViewHandle, WindowId,
};

use crate::{
    cloud_object::model::persistence::ObjectStoreModel,
    editor::InteractionState,
    notebooks::editor::keys::NotebookKeybindings,
    settings_view::keybindings::KeybindingChangedNotifier,
    test_util::settings::initialize_settings_for_tests,
    vim_registers::VimRegisters,
    workspace::{sync_inputs::SyncedInputState, ActiveSession},
    workspaces::user_workspaces::UserWorkspaces,
    AuthStateProvider,
};

use super::{CodeEditorRenderOptions, CodeEditorView, CodeEditorViewAction};
use warp_util::user_input::UserInput;

/// The settings and singleton models a `CodeEditorView` needs in a test app; shared with the AI
/// block's code-block recycling tests (#1267).
pub(crate) fn init_code_editor_test_singletons(app: &mut App) {
    initialize_settings_for_tests(app);

    // Add all required singleton models for EditorView dependencies
    app.add_singleton_model(|_| Appearance::mock());
    app.add_singleton_model(|_| SyncedInputState::mock());
    app.add_singleton_model(|_| VimRegisters::new());
    app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());

    // Add mocks required by rich text editor (used in CommentEditor)
    app.add_singleton_model(ObjectStoreModel::mock);
    app.add_singleton_model(|_| ActiveSession::default());
    app.add_singleton_model(NotebookKeybindings::new);

    // Add UserWorkspaces mock (required by EditorView)
    app.add_singleton_model(|ctx| UserWorkspaces::mock(vec![], ctx));
}

fn initialize_editor(app: &mut App) -> (WindowId, ViewHandle<CodeEditorView>) {
    init_code_editor_test_singletons(app);

    let (window, editor_view) = app.add_window(WindowStyle::NotStealFocus, |ctx| {
        CodeEditorView::new(
            None,
            None,
            CodeEditorRenderOptions::new(VerticalExpansionBehavior::GrowToMaxHeight),
            ctx,
        )
        .with_horizontal_scrollbar_appearance(ScrollableAppearance::new(ScrollbarWidth::Auto, true))
    });

    (window, editor_view)
}

#[test]
fn test_interaction_state_prevents_editing() {
    App::test((), |mut app| async move {
        let (_window, editor_view) = initialize_editor(&mut app);

        let text = editor_view.update(&mut app, |view, ctx| {
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("abc")), ctx);
            view.text(ctx)
        });

        assert_eq!(text.as_str(), "abc");

        // Set to be only selectable
        editor_view.update(&mut app, |view, ctx| {
            view.set_interaction_state(InteractionState::Selectable, ctx);
        });

        let text = editor_view.update(&mut app, |view, ctx| {
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("def")), ctx);
            view.text(ctx)
        });

        assert_eq!(text.as_str(), "abc");
    });
}

/// #1380: keys typed while a paste is being read off the UI thread are held and applied after the
/// pasted text, in typing order.
#[test]
fn test_keys_typed_during_a_paste_read_apply_after_it_in_order() {
    App::test((), |mut app| async move {
        let (_window, editor_view) = initialize_editor(&mut app);
        let text = editor_view.update(&mut app, |view, ctx| {
            let id = view.paste_gate.begin();
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("x")), ctx);
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("y")), ctx);
            assert_eq!(view.text(ctx).as_str(), "", "typed keys wait for the paste");
            view.finish_paste(
                id,
                warpui::clipboard::BackgroundTextRead::Content(
                    warpui::clipboard::ClipboardContent::plain_text("PASTE".into()),
                ),
                ctx,
            );
            view.text(ctx)
        });
        assert_eq!(text.as_str(), "PASTExy");
    });
}

/// #1380: a read that failed pastes nothing and still applies the held keys; a second paste asked
/// for during the read queues behind it.
#[test]
fn test_failed_paste_read_keeps_the_keys_in_the_code_editor() {
    App::test((), |mut app| async move {
        let (_window, editor_view) = initialize_editor(&mut app);
        let text = editor_view.update(&mut app, |view, ctx| {
            let id = view.paste_gate.begin();
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("a")), ctx);
            view.handle_action(&CodeEditorViewAction::Paste, ctx);
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("b")), ctx);
            view.finish_paste(id, warpui::clipboard::BackgroundTextRead::TimedOut, ctx);
            view.text(ctx)
        });
        assert_eq!(text.as_str(), "ab");
    });
}

/// #1409: on Linux the code editor's `Paste` custom action is `ctrl-shift-v`, so plain `ctrl-v`
/// needs the compensating `WindowsPaste` binding in the `CodeEditorView` context.
#[test]
#[cfg(any(windows, target_os = "linux"))]
fn test_ctrl_v_pastes_in_the_code_editor() {
    use crate::util::bindings::CustomAction;
    use warpui::keymap::Context;

    App::test((), |mut app| async move {
        app.update(super::init);
        let mut context = Context::default();
        context.set.insert("CodeEditorView");
        let bound = app.read(|ctx| {
            ctx.binding_for_custom_action(CustomAction::WindowsPaste.into(), vec![context])
                .is_some()
        });
        assert!(bound, "ctrl-v must be bound in CodeEditorView");
    });
}
