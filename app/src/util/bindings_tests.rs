use warpui::platform::OperatingSystem;
use warpui::{
    App,
    actions::StandardAction,
    keymap::{EditableBinding, Keystroke, Trigger},
};

use crate::{
    terminal,
    util::bindings::{
        CONTROL_CHARACTER_KEY_REGEX, INSERT_LAST_WORD_BINDING_NAME,
        MOVE_BACKWARD_ONE_WORD_BINDING_NAME, MOVE_FORWARD_ONE_WORD_BINDING_NAME,
        QUIT_APP_BINDING_NAME, alt_word_keys_enabled_from, default_insert_last_word_keystroke,
        default_quit_keystroke, keybinding_name_to_display_string,
        linux_or_windows_word_motion_keystrokes, trigger_to_keystroke,
    },
    workspace::WorkspaceAction,
};

#[cfg(any(windows, target_os = "linux"))]
use crate::util::bindings::{CustomAction, custom_tag_to_keystroke};

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

// #1989 (owner decision 2026-11-10): Alt-. inserts the last word of the previous command, the
// editor's counterpart of readline's yank-last-arg. Off macOS that is the Alt key itself (it is
// the Meta key there); on macOS it is Option-. with Option set to act as Meta, so an Option key
// that types characters keeps typing them.
#[test]
fn test_default_insert_last_word_keystroke_per_platform() {
    let expected = if OperatingSystem::get().is_mac() {
        Keystroke::parse("meta-.").ok()
    } else {
        Keystroke::parse("alt-.").ok()
    };
    assert!(expected.is_some());
    assert_eq!(default_insert_last_word_keystroke(), expected);
}

#[test]
fn test_insert_last_word_binding_default_and_no_collision() {
    App::test((), |mut app| async move {
        crate::workspace::view::tests::initialize_app(&mut app);
        app.update(terminal::init);
        app.update(crate::editor::init);

        app.update(|ctx| {
            let binding = ctx
                .editable_bindings()
                .find(|binding| binding.name == INSERT_LAST_WORD_BINDING_NAME)
                .expect("the insert-last-word binding should be registered as editable");
            assert_eq!(
                trigger_to_keystroke(binding.trigger),
                default_insert_last_word_keystroke()
            );

            // The names of the other bindings whose default is `keystroke` (a fixed binding has
            // an empty name).
            let claimants = |keystroke: &str| -> Vec<String> {
                let keystroke = Keystroke::parse(keystroke).ok();
                ctx.get_key_bindings()
                    .filter(|binding| binding.name != INSERT_LAST_WORD_BINDING_NAME)
                    .filter(|binding| trigger_to_keystroke(binding.trigger) == keystroke)
                    .map(|binding| binding.name.to_string())
                    .collect()
            };
            let none = Vec::<String>::new();

            // Alt-. is the editable binding's alone.
            assert_eq!(claimants("alt-."), none);
            // `meta-.`: the editable binding on macOS; elsewhere exactly the one fixed binding
            // that keeps an Alt key set to act as Meta working.
            if OperatingSystem::get().is_mac() {
                assert_eq!(claimants("meta-."), none);
            } else {
                assert_eq!(claimants("meta-."), vec![String::new()]);
            }
            // #1988: Alt-R / Meta-R stay unbound, so a program that has the terminal receives
            // them (`ESC r`) and they remain free for the owner's decision on the shell's
            // history search.
            assert_eq!(claimants("alt-r"), none);
            assert_eq!(claimants("meta-r"), none);
        });
    });
}

// #2079: Alt-F / Alt-B / Alt-Backspace are the word keys of the input editor off macOS, as Alt-.
// is the last-argument key (#1989). `PHOSPHOR_ALT_WORD_KEYS=0` is the only value that turns them
// off.
#[test]
fn test_alt_word_keys_kill_switch_is_off_only_for_zero() {
    assert!(alt_word_keys_enabled_from(None));
    assert!(alt_word_keys_enabled_from(Some("")));
    assert!(alt_word_keys_enabled_from(Some("1")));
    assert!(!alt_word_keys_enabled_from(Some("0")));
    assert!(!alt_word_keys_enabled_from(Some(" 0 ")));

    assert_eq!(
        linux_or_windows_word_motion_keystrokes(true),
        ("alt-f", "alt-b")
    );
    // With the kill switch the defaults are the ones every release before #2079 had.
    assert_eq!(
        linux_or_windows_word_motion_keystrokes(false),
        ("meta-f", "meta-b")
    );
}

#[test]
fn test_word_keys_default_bindings_per_platform() {
    App::test((), |mut app| async move {
        crate::workspace::view::tests::initialize_app(&mut app);
        app.update(terminal::init);
        app.update(crate::editor::init);

        app.update(|ctx| {
            // The names of every binding whose default is `keystroke`, sorted (a fixed binding
            // has an empty name).
            let claimants = |keystroke: &str| -> Vec<String> {
                let keystroke = Keystroke::parse(keystroke).ok();
                assert!(keystroke.is_some());
                let mut names: Vec<String> = ctx
                    .get_key_bindings()
                    .filter(|binding| trigger_to_keystroke(binding.trigger) == keystroke)
                    .map(|binding| binding.name.to_string())
                    .collect();
                names.sort();
                names
            };
            let fixed = String::new();
            let forward = MOVE_FORWARD_ONE_WORD_BINDING_NAME.to_string();
            let backward = MOVE_BACKWARD_ONE_WORD_BINDING_NAME.to_string();
            // The code and notebook editors register bindings of the same two names for their
            // own views, so a keystroke may be claimed by the name more than once.
            let only = |names: &[String], name: &String| {
                !names.is_empty() && names.iter().all(|claimant| claimant == name)
            };

            if OperatingSystem::get().is_mac() {
                // Option as Meta: the editable binding and the fixed one, as before #2079. A
                // plain Option-F / Option-B types the layout's character and is not bound.
                assert!(claimants("meta-f").contains(&forward));
                assert!(claimants("meta-f").contains(&fixed));
                assert!(claimants("meta-b").contains(&backward));
                assert!(claimants("meta-b").contains(&fixed));
                assert_eq!(claimants("alt-f"), Vec::<String>::new());
                assert_eq!(claimants("alt-b"), Vec::<String>::new());
            } else {
                // The Alt key itself, and nothing else claims it.
                assert!(only(&claimants("alt-f"), &forward), "alt-f");
                assert!(only(&claimants("alt-b"), &backward), "alt-b");
                // An Alt key set to act as Meta keeps the keys it had: fixed bindings only.
                assert!(only(&claimants("meta-f"), &fixed), "meta-f");
                assert!(only(&claimants("meta-b"), &fixed), "meta-b");
                // Alt-Backspace cuts the word before the cursor, next to `meta-backspace`.
                assert!(only(&claimants("alt-backspace"), &fixed), "alt-backspace");
                assert!(only(&claimants("meta-backspace"), &fixed), "meta-backspace");
            }
        });
    });
}
