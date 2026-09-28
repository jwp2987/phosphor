use warpui::platform::OperatingSystem;
use warpui::{
    actions::StandardAction,
    keymap::{EditableBinding, Keystroke, Trigger},
    App,
};

use crate::{
    terminal,
    util::bindings::{
        CONTROL_CHARACTER_KEY_REGEX, QUIT_APP_BINDING_NAME, default_quit_keystroke,
        keybinding_name_to_display_string, trigger_to_keystroke,
    },
    workspace::WorkspaceAction,
};

#[cfg(any(windows, target_os = "linux"))]
use crate::util::bindings::{custom_tag_to_keystroke, CustomAction};

#[test]
fn test_keybinding_name_to_display_string() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.register_editable_bindings([
                EditableBinding::new(
                    "workspace:show_settings",
                    "Open settings",
                    WorkspaceAction::ShowSettings,
                )
                .with_key_binding("cmd-,"),
                EditableBinding::new(
                    "workspace:toggle_resource_center",
                    "Toggle Resource Center",
                    WorkspaceAction::ToggleResourceCenter,
                ),
            ]);

            let displayed_keybinding = if OperatingSystem::get().is_mac() {
                "⌘,"
            } else {
                "Logo ,"
            };
            assert_eq!(
                Some(displayed_keybinding),
                keybinding_name_to_display_string("workspace:show_settings", ctx).as_deref()
            );

            assert_eq!(
                None,
                keybinding_name_to_display_string("workspace:toggle_resource_center", ctx)
            );

            ctx.set_custom_trigger(
                "workspace:show_settings".to_owned(),
                Trigger::Keystrokes(vec![Keystroke::parse("cmd-shift-<").unwrap()]),
            );

            let displayed_keybinding = if OperatingSystem::get().is_mac() {
                "⇧⌘<"
            } else {
                "Shift Logo <"
            };
            assert_eq!(
                Some(displayed_keybinding),
                keybinding_name_to_display_string("workspace:show_settings", ctx).as_deref()
            );

            ctx.set_custom_trigger(
                "workspace:toggle_resource_center".to_owned(),
                Trigger::Keystrokes(vec![Keystroke::parse("cmd-alt-/").unwrap()]),
            );

            let expected_keybinding = if OperatingSystem::get().is_mac() {
                "⌥⌘/"
            } else {
                "Alt Logo /"
            };
            assert_eq!(
                Some(expected_keybinding),
                keybinding_name_to_display_string("workspace:toggle_resource_center", ctx)
                    .as_deref()
            );
        });
    });
}

// Ported from the pin (Warp 2026.07.29.09.05, commit 02b53fcd8)
// `app/src/util/bindings_tests.rs::test_toggle_maximize_pane_binding_is_editable`,
// tracked as issue #410. Deviation from the pin (AGENTS.md §5.10): the non-mac
// default assertion is `ctrl-alt-m`, not `None`. The fork's
// `CustomAction::ToggleMaximizePane` (see `app/src/util/bindings.rs`) deliberately
// gives Linux/Windows a default of `ctrl-alt-m` instead of leaving it unbound,
// because `ctrl-shift-enter` / `alt-shift-enter` / `ctrl-alt-enter` are all already
// claimed by prompt-suggestion bindings on those platforms. That divergence
// predates this port and is intentional, so the test is adapted to match actual
// (better) fork behavior rather than weakened to match the pin.
#[test]
fn test_toggle_maximize_pane_binding_is_editable() {
    App::test((), |mut app| async move {
        app.update(crate::pane_group::init);

        app.update(|ctx| {
            use crate::pane_group::TOGGLE_MAXIMIZE_PANE_BINDING_NAME;

            // The toggle-maximize-pane action is registered as an editable binding so
            // it can be assigned a shortcut in Settings → Keyboard shortcuts.
            assert!(
                ctx.editable_bindings()
                    .any(|binding| binding.name == TOGGLE_MAXIMIZE_PANE_BINDING_NAME),
                "{TOGGLE_MAXIMIZE_PANE_BINDING_NAME} should be registered as an editable binding"
            );

            // It ships with a mac-only default shortcut (cmd-shift-enter) via its custom
            // action; other platforms default to ctrl-alt-m (see the module comment above
            // for why). Either way, whatever resolves here is what the pane header menu
            // item surfaces.
            let default = keybinding_name_to_display_string(TOGGLE_MAXIMIZE_PANE_BINDING_NAME, ctx);
            if OperatingSystem::get().is_mac() {
                assert_eq!(Some("⇧⌘⏎"), default.as_deref());
            } else {
                assert_eq!(Some("Ctrl Alt M"), default.as_deref());
            }

            // A reassigned shortcut resolves to its display string on every platform.
            ctx.set_custom_trigger(
                TOGGLE_MAXIMIZE_PANE_BINDING_NAME.to_owned(),
                Trigger::Keystrokes(vec![Keystroke::parse("cmd-shift-M").unwrap()]),
            );

            let displayed_keybinding = if OperatingSystem::get().is_mac() {
                "⇧⌘M"
            } else {
                "Shift Logo M"
            };
            assert_eq!(
                Some(displayed_keybinding),
                keybinding_name_to_display_string(TOGGLE_MAXIMIZE_PANE_BINDING_NAME, ctx)
                    .as_deref()
            );
        });
    });
}

/// The pane header overflow menu's "Move pane to its own tab" item
/// (`PaneGroupAction::MoveFocusedPaneToOwnTab`) is unbound by default -- no
/// existing default keystroke was free on either platform (see
/// `CustomAction::ToggleMaximizePane`'s module comment above for the
/// Linux/Windows keys that are already spoken for) -- but it is registered as
/// an editable binding so a shortcut can still be assigned in Settings ->
/// Keyboard shortcuts, same as every other pane-header-overflow-menu item that
/// is bindable at all.
#[test]
fn test_move_pane_to_own_tab_binding_is_editable() {
    App::test((), |mut app| async move {
        app.update(crate::pane_group::init);

        app.update(|ctx| {
            use crate::pane_group::MOVE_PANE_TO_OWN_TAB_BINDING_NAME;

            assert!(
                ctx.editable_bindings()
                    .any(|binding| binding.name == MOVE_PANE_TO_OWN_TAB_BINDING_NAME),
                "{MOVE_PANE_TO_OWN_TAB_BINDING_NAME} should be registered as an editable binding"
            );

            assert_eq!(
                None,
                keybinding_name_to_display_string(MOVE_PANE_TO_OWN_TAB_BINDING_NAME, ctx),
                "should ship unbound by default"
            );

            // A reassigned shortcut resolves to its display string on every platform.
            ctx.set_custom_trigger(
                MOVE_PANE_TO_OWN_TAB_BINDING_NAME.to_owned(),
                Trigger::Keystrokes(vec![Keystroke::parse("cmd-shift-O").unwrap()]),
            );

            let displayed_keybinding = if OperatingSystem::get().is_mac() {
                "⇧⌘O"
            } else {
                "Shift Logo O"
            };
            assert_eq!(
                Some(displayed_keybinding),
                keybinding_name_to_display_string(MOVE_PANE_TO_OWN_TAB_BINDING_NAME, ctx)
                    .as_deref()
            );
        });
    });
}

#[test]
fn test_terminal_page_scroll_bindings_are_editable() {
    App::test((), |mut app| async move {
        app.update(terminal::init);

        app.update(|ctx| {
            let page_up = ctx
                .editable_bindings()
                .find(|binding| binding.name == "terminal:scroll_up_one_page")
                .and_then(|binding| trigger_to_keystroke(binding.trigger));
            let page_down = ctx
                .editable_bindings()
                .find(|binding| binding.name == "terminal:scroll_down_one_page")
                .and_then(|binding| trigger_to_keystroke(binding.trigger));

            assert_eq!(page_up, Keystroke::parse("pageup").ok());
            assert_eq!(page_down, Keystroke::parse("pagedown").ok());
        });
    });
}

// Regression test for https://github.com/zerx-lab/zap/issues/303: `EditorView`'s default Paste
// binding is `ctrl-shift-v` on non-Mac (see `cmd_or_ctrl_shift`), so Linux (like Windows) needs a
// compensating plain `ctrl-v` binding via `CustomAction::WindowsPaste`.
#[test]
#[cfg(any(windows, target_os = "linux"))]
fn test_windows_paste_custom_action_binds_to_plain_ctrl_v() {
    let expected = Keystroke::parse("ctrl-v").expect("\"ctrl-v\" should be a valid keystroke");
    assert_eq!(
        custom_tag_to_keystroke(CustomAction::WindowsPaste.into()),
        Some(expected)
    );
}

// Linux/Windows had no quit shortcut: the pin (and this fork until now) only quit via the macOS
// menu's `cmd-q`, and `workspace:terminate_app` ("Quit Phosphor") shipped without a default key.
// Off macOS the default is `ctrl-shift-Q`, never `ctrl-q` (XON / readline quoted-insert, which
// must reach the shell). See DECLINED.md (IMPROVED).
#[test]
fn test_default_quit_keystroke_per_platform() {
    let expected = if OperatingSystem::get().is_mac() {
        Keystroke::parse("cmd-q").ok()
    } else {
        Keystroke::parse("ctrl-shift-Q").ok()
    };
    assert!(expected.is_some());
    assert_eq!(default_quit_keystroke(), expected);
    // The standard Quit action displays the same shortcut the keymap binds.
    assert_eq!(
        trigger_to_keystroke(&Trigger::Standard(StandardAction::Quit)),
        expected
    );
}

#[test]
fn test_default_quit_keystroke_is_not_a_control_character() {
    let keystroke = default_quit_keystroke().expect("quit keystroke should parse");
    assert!(
        !CONTROL_CHARACTER_KEY_REGEX.is_match(keystroke.normalized().as_str()),
        "the quit shortcut must not swallow a control character the shell needs: {}",
        keystroke.normalized()
    );
}

#[test]
fn test_quit_binding_default_and_no_collision() {
    App::test((), |mut app| async move {
        crate::workspace::view::tests::initialize_app(&mut app);

        app.update(|ctx| {
            let quit = ctx
                .editable_bindings()
                .find(|binding| binding.name == QUIT_APP_BINDING_NAME)
                .expect("the Quit binding should be registered as an editable binding");
            let quit_keystroke = trigger_to_keystroke(quit.trigger);

            if OperatingSystem::get().is_mac() {
                // macOS quits through the native menu's `cmd-q`; the keymap binding may only
                // mirror it, never claim something else.
                assert!(
                    quit_keystroke.is_none() || quit_keystroke == default_quit_keystroke(),
                    "unexpected mac Quit keystroke: {quit_keystroke:?}"
                );
            } else {
                assert_eq!(quit_keystroke, default_quit_keystroke());
            }

            // Nothing else in the default keymap claims the quit shortcut.
            let default_quit = default_quit_keystroke();
            let claimants: Vec<&str> = ctx
                .get_key_bindings()
                .filter(|binding| binding.name != QUIT_APP_BINDING_NAME)
                .filter(|binding| trigger_to_keystroke(binding.trigger) == default_quit)
                .map(|binding| binding.name)
                .collect();
            assert!(
                claimants.is_empty(),
                "the default quit shortcut collides with: {claimants:?}"
            );
        });
    });
}
