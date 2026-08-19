//! GUI usage / acceptance smoke scenarios (Chunk B of the usage-test suite).
//!
//! These `usage_*` scenarios are a thin *curation* layer over the existing
//! integration steps, getters, and assertions (see `app/src/integration_testing/`
//! and `crate::test`). They introduce **no new assertion infrastructure** — each
//! scenario simply composes proven building blocks into a higher-level "does the
//! running app actually do the thing" flow.
//!
//! Tagging (mirrored in `crates/usage_suite/src/manifest.rs`):
//! * **reliable-here** — in-process, no real shell / provider / GPU display; these
//!   assert view/model/injection state and are trustworthy in the sandbox.
//! * **needs-real-shell** — drives a real PTY shell to command completion, which
//!   is subject to the bash-preexec race in this sandbox (blocks can stick at
//!   `DoneWithNoExecution`); run only with `--include-flaky`, auto-retried.
//! * **needs-byop-provider** — a genuine agent round-trip needs a real key +
//!   network; skipped unless `--include-byop`. Until the Chunk-D provider mock
//!   lands, the scenario exercises the agent *UI* via synthetic injection.
//! * **needs-desktop** — wants a real GPU window / pixel geometry; skipped here.
//!
//! Registration: like every integration test, each scenario must also be listed
//! in `crate::test` (re-export) and in `register_tests()` in
//! `crates/integration/src/bin/integration.rs`.

use warp::{
    cmd_or_ctrl_shift,
    integration_testing::{
        command_palette::{
            close_command_palette, open_command_palette, open_command_palette_and_run_action,
            TestStepsExt,
        },
        input::input_editor_is_focused,
        secret_redaction::assert_secrets_redacted_for_ai,
        settings::{
            set_window_custom_size, toggle_hide_secrets_in_block_list_setting,
            toggle_safe_mode_setting,
        },
        step::new_step_with_default_assertions,
        tab::{assert_pane_title, assert_tab_title},
        terminal::{
            assert_selected_block_index_is_last_renderable, clear_blocklist_to_remove_bootstrapped_blocks,
            execute_command_for_single_terminal_in_tab, initialize_secret_regexes,
            util::ExpectedExitStatus, wait_until_bootstrapped_single_pane_for_tab,
        },
        view_getters::single_terminal_view_for_tab,
        workspace::assert_tab_count,
    },
    settings_view::{SettingsSection, SettingsView},
    workspace::{Workspace, NEW_TAB_BUTTON_POSITION_ID},
};
use warpui::{async_assert, async_assert_eq, integration::TestStep, ViewHandle};

use super::{new_builder, Builder};
use crate::util::skip_if_powershell_core_2303;

/// The synthetic user query used by the injected dummy AI blocks.
const DUMMY_AI_QUERY: &str = "Produce some dummy output for the usage suite";
/// Markdown title inside the injected AI output (validates header rendering).
const DUMMY_AI_TITLE: &str = "Usage Suite Dummy Title";
/// A distinctive token in the AI output body, used by the find scenario.
const DUMMY_AI_BODY_TOKEN: &str = "findabletoken";

/// Markdown body for the injected dummy AI block. The `###` header and `*` list
/// items exercise markdown rendering; `findabletoken` is a search target.
fn dummy_ai_output() -> String {
    format!(
        concat!(
            "### {title}\n",
            "* This dummy AI output is {token} by the usage suite.\n",
            "* This is the second list item."
        ),
        title = DUMMY_AI_TITLE,
        token = DUMMY_AI_BODY_TOKEN,
    )
}

/// A step that synthetically injects a dummy AI block (query + markdown output)
/// into the single terminal of tab 0. This is the sanctioned no-provider path
/// for exercising agent UI (`TerminalView::insert_dummy_ai_block`) — see
/// `crate::test::agent_mode`.
fn insert_dummy_ai_block_step() -> TestStep {
    new_step_with_default_assertions("Inject a synthetic AI block").with_action(|app, _, _| {
        let window_id = app.window_ids()[0];
        let terminal_view = single_terminal_view_for_tab(app, window_id, 0);
        terminal_view.update(app, |view, ctx| {
            view.insert_dummy_ai_block(DUMMY_AI_QUERY.to_owned(), dummy_ai_output(), ctx);
        });
    })
}

/// `usage_launch_bootstrap` (reliable-here): launching the app bootstraps a
/// single pane for the first tab and leaves the input editor focused.
pub fn usage_launch_bootstrap() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(
            new_step_with_default_assertions("Input editor is focused after bootstrap")
                .add_assertion(input_editor_is_focused(0)),
        )
}

/// `usage_open_close_settings` (reliable-here): the settings pane opens in a new
/// tab and can be closed again. Mirrors `test_open_and_close_settings`.
pub fn usage_open_close_settings() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(
            new_step_with_default_assertions("Open settings tab")
                .with_keystrokes(&["cmdorctrl-,"])
                .add_assertion(assert_tab_count(2))
                .add_assertion(assert_tab_title(1, "Settings"))
                .add_assertion(assert_pane_title(1, 0, "Settings"))
                .add_assertion(|app, window_id| {
                    let settings_views: Vec<ViewHandle<SettingsView>> = app
                        .views_of_type(window_id)
                        .expect("Settings view must exist");
                    let settings_view = settings_views.first().expect("Settings view must exist");
                    settings_view.read(app, |view, _| {
                        async_assert_eq!(
                            view.current_settings_section(),
                            SettingsSection::default()
                        )
                    })
                }),
        )
        .with_step(
            new_step_with_default_assertions("Close the settings tab with the close-tab button")
                .with_hover_over_saved_position("close_tab_button:1")
                .with_click_on_saved_position("close_tab_button:1")
                // The subject of this smoke is the settings tab opening and
                // closing, asserted by the tab count returning to 1. We do not
                // assert the remaining terminal tab's title here: a home shell's
                // "~" title is derived from shell integration (the same
                // cwd/prompt machinery behind the `needs-real-shell` scenarios),
                // which is not reliable in this headless sandbox.
                .add_assertion(assert_tab_count(1)),
        )
}

/// `usage_open_command_palette` (reliable-here): the command palette opens,
/// surfaces results while typing, and an entry can be executed. Running "Open
/// Theme Picker" through the palette deterministically opens the theme chooser
/// (no filesystem / shell side effects). Reuses the `command_palette/` steps.
pub fn usage_open_command_palette() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        // Open and close once to assert the palette open/close lifecycle.
        .with_step(open_command_palette())
        .with_step(close_command_palette())
        // Open the palette again and run an entry, asserting its effect.
        .with_steps(
            open_command_palette_and_run_action("Open Theme Picker").add_assertion(
                |app, window_id| {
                    let views: Vec<ViewHandle<Workspace>> =
                        app.views_of_type(window_id).expect("workspace must exist");
                    let workspace = views.first().expect("workspace must exist");
                    workspace.read(app, |view, _| {
                        async_assert!(
                            view.is_theme_chooser_open(),
                            "Theme chooser should be open after running the palette entry"
                        )
                    })
                },
            ),
        )
}

/// `usage_tabs_add_switch_close` (reliable-here): add tabs, switch the active
/// tab, and close a tab; assert tab count and which tab is active. Mirrors
/// `test_removing_tabs_out_of_order` (button click + keybinding driven).
pub fn usage_tabs_add_switch_close() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(
            new_step_with_default_assertions("Add a second tab with the new-tab button")
                .with_click_on_saved_position(NEW_TAB_BUTTON_POSITION_ID)
                .add_assertion(assert_tab_count(2)),
        )
        .with_step(wait_until_bootstrapped_single_pane_for_tab(1))
        .with_step(
            new_step_with_default_assertions("Add a third tab with the new-tab button")
                .with_click_on_saved_position(NEW_TAB_BUTTON_POSITION_ID)
                .add_assertion(assert_tab_count(3)),
        )
        .with_step(wait_until_bootstrapped_single_pane_for_tab(2))
        .with_step(
            new_step_with_default_assertions("Switch to the first tab and assert it is active")
                .with_keystrokes(&["cmdorctrl-1"])
                .add_assertion(|app, window_id| {
                    let views: Vec<ViewHandle<Workspace>> = app.views_of_type(window_id).unwrap();
                    let workspace = views.first().unwrap();
                    let (active_tab_id, first_tab_id) = workspace.read(app, |workspace, _| {
                        (
                            workspace.active_tab_pane_group().id(),
                            workspace.get_pane_group_view_unchecked(0).id(),
                        )
                    });
                    async_assert_eq!(
                        active_tab_id,
                        first_tab_id,
                        "Expected the first tab (ID {}) to be active, but active was (ID {})",
                        first_tab_id,
                        active_tab_id,
                    )
                }),
        )
        .with_step(
            new_step_with_default_assertions("Close the active tab")
                .with_keystrokes(&[cmd_or_ctrl_shift("w")])
                .add_assertion(assert_tab_count(2)),
        )
        .with_step(
            new_step_with_default_assertions("Close another tab")
                .with_keystrokes(&[cmd_or_ctrl_shift("w")])
                .add_assertion(assert_tab_count(1)),
        )
}

/// `usage_theme_creator_modal` (reliable-here): open the theme picker, open the
/// theme creator modal, then close it. Mirrors
/// `test_open_and_close_theme_creator_modal`.
pub fn usage_theme_creator_modal() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_steps(
            open_command_palette_and_run_action("Open Theme Picker").add_assertion(
                |app, window_id| {
                    let views: Vec<ViewHandle<Workspace>> = app.views_of_type(window_id).unwrap();
                    let workspace = views.first().unwrap();
                    workspace.read(app, |view, _| {
                        async_assert!(view.is_theme_chooser_open(), "Theme chooser should be open")
                    })
                },
            ),
        )
        .with_step(
            new_step_with_default_assertions("Open the theme creator modal")
                .with_click_on_saved_position("create_theme_button")
                .add_assertion(|app, window_id| {
                    let views: Vec<ViewHandle<Workspace>> = app.views_of_type(window_id).unwrap();
                    let workspace = views.first().unwrap();
                    workspace.read(app, |view, _| {
                        async_assert!(
                            view.is_theme_creator_modal_open(),
                            "Theme creator modal should be open"
                        )
                    })
                }),
        )
        .with_step(
            new_step_with_default_assertions("Close the theme creator modal")
                .with_click_on_saved_position("theme_creator_cancel_button")
                .add_assertion(|app, window_id| {
                    let views: Vec<ViewHandle<Workspace>> = app.views_of_type(window_id).unwrap();
                    let workspace = views.first().unwrap();
                    workspace.read(app, |view, _| {
                        async_assert!(
                            !view.is_theme_creator_modal_open(),
                            "Theme creator modal should be closed"
                        )
                    })
                }),
        )
}

/// `usage_block_navigation_select` (needs-real-shell): after a command runs and
/// produces a block, the last block can be selected with the block-navigation
/// keybinding. Block selection operates over the real block list, so it needs a
/// genuine command block (a synthetically injected AI block is not a selectable
/// participant) — hence a real shell, which is subject to the bash-preexec race
/// in this sandbox. Mirrors `ai_assistant::test_ask_warp_ai_keybinding_for_selected_block`.
pub fn usage_block_navigation_select() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(execute_command_for_single_terminal_in_tab(
            0,
            "echo hello".to_string(),
            ExpectedExitStatus::Success,
            "hello",
        ))
        .with_step(
            new_step_with_default_assertions("Select the last block")
                .with_keystrokes(&["cmdorctrl-up"])
                .add_named_assertion(
                    "last block is selected",
                    assert_selected_block_index_is_last_renderable(),
                ),
        )
}

/// `usage_find_in_block` (needs-real-shell): open the find bar over real command
/// output, type a query, and assert a match is found. The find model searches
/// the real block list, so this needs a genuine command block (a synthetically
/// injected AI block is not searched) — hence a real shell, which is subject to
/// the bash-preexec race in this sandbox. Mirrors the find pattern in
/// `block_filtering`.
pub fn usage_find_in_block() -> Builder {
    // A distinctive token echoed into the command output and then searched for.
    const FIND_TOKEN: &str = "findableoutput";
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(execute_command_for_single_terminal_in_tab(
            0,
            format!("echo {FIND_TOKEN}"),
            ExpectedExitStatus::Success,
            FIND_TOKEN,
        ))
        .with_step(
            new_step_with_default_assertions("Open the find bar")
                .with_keystrokes(&[cmd_or_ctrl_shift("f")])
                .add_assertion(|app, window_id| {
                    let terminal_view = single_terminal_view_for_tab(app, window_id, 0);
                    let (is_open, is_focused) = terminal_view.read(app, |view, ctx| {
                        (view.is_find_bar_open(ctx), view.is_find_bar_focused(ctx))
                    });
                    async_assert!(
                        is_open && is_focused,
                        "Expected the find bar to be open and focused"
                    )
                }),
        )
        .with_step(
            new_step_with_default_assertions("Type a query present in the block output")
                .with_typed_characters(&[FIND_TOKEN])
                .add_assertion(|app, window_id| {
                    let terminal_view = single_terminal_view_for_tab(app, window_id, 0);
                    let num_matches = terminal_view.read(app, |view, ctx| {
                        view.find_model().as_ref(ctx).visible_block_list_match_count()
                    });
                    async_assert!(
                        num_matches >= 1,
                        "Expected at least one find match but got {num_matches}"
                    )
                }),
        )
}

/// `usage_agent_block_render` (reliable-here): a synthetically injected AI block
/// renders its title and body (including markdown) into the block list, with no
/// provider involved. Reuses the `agent_mode` injection pattern.
pub fn usage_agent_block_render() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(clear_blocklist_to_remove_bootstrapped_blocks())
        .with_step(insert_dummy_ai_block_step())
        .with_step(
            new_step_with_default_assertions("Injected AI block renders title and body")
                .add_assertion(|app, window_id| {
                    let terminal_view = single_terminal_view_for_tab(app, window_id, 0);
                    terminal_view.read(app, |view, ctx| {
                        let Some(ai_block) = view.last_ai_block() else {
                            return async_assert!(false, "An AI block should exist");
                        };
                        ai_block.read(ctx, |ai_block, _| {
                            let output = ai_block.get_output_text(ctx);
                            async_assert!(
                                output.contains(DUMMY_AI_TITLE)
                                    && output.contains(DUMMY_AI_BODY_TOKEN),
                                "AI block output should render the markdown title and body, got: {output:?}"
                            )
                        })
                    })
                }),
        )
}

/// `usage_run_command_output_block` (needs-real-shell): `echo hello` runs to
/// completion and produces a block whose output is `hello`. Depends on the
/// bash-preexec completion flow, which is racy in this sandbox.
pub fn usage_run_command_output_block() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(execute_command_for_single_terminal_in_tab(
            0,
            "echo hello".to_string(),
            ExpectedExitStatus::Success,
            "hello",
        ))
}

/// `usage_run_command_exit_code` (needs-real-shell): a failing command produces a
/// block in a non-zero exit state. Depends on the (racy) real-shell completion.
pub fn usage_run_command_exit_code() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(execute_command_for_single_terminal_in_tab(
            0,
            "false".to_string(),
            ExpectedExitStatus::Failure,
            (),
        ))
}

/// `usage_secret_redaction` (needs-real-shell): a command whose output contains
/// secrets has those secrets redacted before being handed to the AI. Producing
/// the secret-bearing output requires running a real command, so this is tagged
/// needs-real-shell. Reuses the `secret_redaction` assertions.
pub fn usage_secret_redaction() -> Builder {
    let phone_number = "123-456-7890";
    let secret_api_key = "sk-1234567890abcdef";
    let expected_redacted_phone = "************";
    let expected_redacted_api_key = "******************";
    let test_command = "echo 'Phone: 123-456-7890 API: sk-1234567890abcdef'.";
    let test_output = "Phone: 123-456-7890 API: sk-1234567890abcdef.";

    new_builder()
        // Same guard the scenario this reuses carries
        // (`secrets::test_secrets_are_always_redacted_in_ai_inputs`): the assertions
        // do not hold under PowerShell, tracked as CORE-2303. The copy dropped it,
        // so on Windows -- where PowerShell is the default shell -- this ran anyway
        // and failed on `echo 'x'.` emitting a newline before the trailing period:
        //   expected "…abcdef."   got "…abcdef\n."
        // That is what made the nightly usage suite red on Windows every night from
        // 2026-08-12. Restoring the guard rather than relaxing the assertion: the
        // assertion is correct, the shell is the known-unsupported one.
        .set_should_run_test(skip_if_powershell_core_2303)
        .with_step(initialize_secret_regexes())
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(toggle_safe_mode_setting())
        .with_step(execute_command_for_single_terminal_in_tab(
            0,
            test_command.to_string(),
            ExpectedExitStatus::Success,
            test_output,
        ))
        .with_step(
            new_step_with_default_assertions("Secrets are redacted for AI (strikethrough mode)")
                .add_assertion(assert_secrets_redacted_for_ai(
                    test_output.to_string(),
                    expected_redacted_phone.to_string(),
                    expected_redacted_api_key.to_string(),
                    phone_number.to_string(),
                    secret_api_key.to_string(),
                )),
        )
        .with_step(toggle_hide_secrets_in_block_list_setting())
        .with_step(
            new_step_with_default_assertions("Secrets are redacted for AI (full obfuscation mode)")
                .add_assertion(assert_secrets_redacted_for_ai(
                    test_output.to_string(),
                    expected_redacted_phone.to_string(),
                    expected_redacted_api_key.to_string(),
                    phone_number.to_string(),
                    secret_api_key.to_string(),
                )),
        )
}

/// `usage_agent_roundtrip` (needs-byop-provider): a real agent prompt →
/// tool-call → result round-trip needs a real provider (a key + network), so it
/// is skipped by default. Until the Chunk-D provider mock lands, this exercises
/// the agent block via synthetic injection (`insert_dummy_ai_block`) so the
/// scenario is not blocked on Chunk D.
pub fn usage_agent_roundtrip() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(clear_blocklist_to_remove_bootstrapped_blocks())
        .with_step(insert_dummy_ai_block_step())
        .with_step(
            new_step_with_default_assertions("Synthetic agent response is present")
                .add_assertion(|app, window_id| {
                    let terminal_view = single_terminal_view_for_tab(app, window_id, 0);
                    terminal_view.read(app, |view, ctx| {
                        let Some(ai_block) = view.last_ai_block() else {
                            return async_assert!(false, "An AI block should exist");
                        };
                        ai_block.read(ctx, |ai_block, _| {
                            let output = ai_block.get_output_text(ctx);
                            async_assert!(
                                output.contains(DUMMY_AI_BODY_TOKEN),
                                "Agent response body should be present, got: {output:?}"
                            )
                        })
                    })
                }),
        )
}

/// `usage_font_size_window_resize` (needs-desktop): changing the font size and
/// opening the window at a custom size should re-layout the terminal geometry.
/// The meaningful geometry outcome requires a real GPU display, so this is
/// tagged needs-desktop and skipped in the sandbox. It requests a real display
/// and reuses `set_window_custom_size` plus the font-size keybindings.
pub fn usage_font_size_window_resize() -> Builder {
    new_builder()
        .with_real_display()
        .with_step(set_window_custom_size(40, 120))
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(
            new_step_with_default_assertions("Increase the font size")
                .with_keystrokes(&["ctrl-shift->"]),
        )
        .with_step(
            new_step_with_default_assertions("Decrease the font size")
                .with_keystrokes(&["ctrl-shift-<"]),
        )
}
