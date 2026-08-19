use warpui::{App, WindowId, async_assert, integration::TestStep};

use crate::features::FeatureFlag;
use crate::integration_testing::tab_group::assertion::group_id_for_tab;
use crate::integration_testing::view_getters::workspace_view;
use crate::workspace::WorkspaceAction;

/// Dispatches a workspace action through the real action pipeline, with the
/// workspace view as the responder — the same route a menu item or keybinding
/// takes.
fn dispatch_workspace_action(app: &mut App, window_id: WindowId, action: &WorkspaceAction) {
    let workspace_view_id = workspace_view(app, window_id).id();
    app.dispatch_typed_action(window_id, &[workspace_view_id], action);
}

/// The group of the tab at `anchor_tab_index`.
///
/// Group ids are freshly generated `Uuid`s, so a test cannot name one up
/// front. Every group-targeting step here identifies its group by a member
/// tab instead.
fn group_id_for_anchor(
    app: &App,
    window_id: WindowId,
    anchor_tab_index: usize,
) -> crate::workspace::tab_group::TabGroupId {
    group_id_for_tab(app, window_id, anchor_tab_index).unwrap_or_else(|| {
        panic!("tab {anchor_tab_index} should belong to a tab group for this step")
    })
}

/// Turns `FeatureFlag::GroupedTabs` on for the rest of the test and asserts it
/// took effect.
///
/// The flag is default-on today, but every grouping code path — the model
/// mutations, `tab_bar_slots`, and the group-membership block in
/// `on_tab_drag` — is gated on it, so a tab-group test that merely assumed it
/// would degrade into a test of ungrouped tabs that still passes. `set_user_preference`
/// is the process-global tier `is_enabled` consults ahead of the compiled-in
/// state (unlike `override_enabled`, which is thread-local and would not hold
/// for the app's UI thread).
pub fn ensure_grouped_tabs_enabled() -> TestStep {
    TestStep::new("Ensure grouped tabs are enabled")
        .with_action(|_app, _window_id, _data| {
            FeatureFlag::GroupedTabs.set_user_preference(true);
        })
        .add_named_assertion("GroupedTabs is enabled", |_app, _window_id| {
            async_assert!(
                FeatureFlag::GroupedTabs.is_enabled(),
                "FeatureFlag::GroupedTabs must be enabled for tab group tests"
            )
        })
}

/// Creates a new tab group containing only the tab at `tab_index`
/// (`WorkspaceAction::NewTabGroupFromTab`, the tab context menu's
/// "New group with tab").
pub fn create_tab_group_from_tab(tab_index: usize) -> TestStep {
    TestStep::new("Create a tab group from a tab").with_action(move |app, window_id, _data| {
        dispatch_workspace_action(
            app,
            window_id,
            &WorkspaceAction::NewTabGroupFromTab(tab_index),
        );
    })
}

/// Adds the tab at `tab_index` to the group that the tab at
/// `anchor_tab_index` belongs to (`WorkspaceAction::MoveTabToGroup`, the tab
/// context menu's "Move to group").
pub fn move_tab_to_group_of_tab(tab_index: usize, anchor_tab_index: usize) -> TestStep {
    TestStep::new("Move a tab into an existing group").with_action(move |app, window_id, _data| {
        let group_id = group_id_for_anchor(app, window_id, anchor_tab_index);
        dispatch_workspace_action(
            app,
            window_id,
            &WorkspaceAction::MoveTabToGroup {
                tab_index,
                group_id,
            },
        );
    })
}

/// Toggles collapse on the group containing the tab at `anchor_tab_index`
/// (`WorkspaceAction::ToggleTabGroupCollapsed`, what clicking the group header
/// dispatches).
pub fn toggle_tab_group_collapsed_of_tab(anchor_tab_index: usize) -> TestStep {
    TestStep::new("Toggle a tab group's collapsed state").with_action(
        move |app, window_id, _data| {
            let group_id = group_id_for_anchor(app, window_id, anchor_tab_index);
            dispatch_workspace_action(
                app,
                window_id,
                &WorkspaceAction::ToggleTabGroupCollapsed(group_id),
            );
        },
    )
}

/// Opens the inline rename editor over the header of the group containing the
/// tab at `anchor_tab_index` (`WorkspaceAction::RenameTabGroup`, what
/// double-clicking the header dispatches). The editor is focused with its
/// current name pre-selected, so the caller types the new name and presses
/// enter to commit.
pub fn rename_tab_group_of_tab(anchor_tab_index: usize) -> TestStep {
    TestStep::new("Open the tab group rename editor").with_action(move |app, window_id, _data| {
        let group_id = group_id_for_anchor(app, window_id, anchor_tab_index);
        dispatch_workspace_action(app, window_id, &WorkspaceAction::RenameTabGroup(group_id));
    })
}

/// Closes the group containing the tab at `anchor_tab_index`, and every tab in
/// it (`WorkspaceAction::CloseTabGroup`, the group menu's "Close group").
pub fn close_tab_group_of_tab(anchor_tab_index: usize) -> TestStep {
    TestStep::new("Close a tab group").with_action(move |app, window_id, _data| {
        let group_id = group_id_for_anchor(app, window_id, anchor_tab_index);
        dispatch_workspace_action(app, window_id, &WorkspaceAction::CloseTabGroup(group_id));
    })
}

/// Opens a new terminal tab inside the group containing the tab at
/// `anchor_tab_index` (`WorkspaceAction::NewTabInGroup`, the group menu's
/// "New tab in group").
pub fn new_tab_in_group_of_tab(anchor_tab_index: usize) -> TestStep {
    TestStep::new("Open a new tab inside a group").with_action(move |app, window_id, _data| {
        let group_id = group_id_for_anchor(app, window_id, anchor_tab_index);
        dispatch_workspace_action(app, window_id, &WorkspaceAction::NewTabInGroup(group_id));
    })
}

/// Opens the user's `settings.toml` in a new tab
/// (`WorkspaceAction::OpenSettingsFile`).
///
/// This is a real "open a file in a new tab" flow: it reaches
/// `Workspace::add_tab_for_code_file`, which takes its insertion index *and*
/// its group from `new_tab_index_and_group`. That is the shared placement path
/// for opening a code file, a notebook, or a plain new tab, and the reason a
/// file opened from inside a group must join that group rather than land
/// ungrouped in the middle of its run.
pub fn open_settings_file_in_new_tab() -> TestStep {
    TestStep::new("Open the settings file in a new tab")
        .with_setup(|_utils| {
            // A fresh integration profile has no `settings.toml`, and this test
            // is about tab placement, not about how the editor renders a
            // missing file. Create it so the tab opens on a real file.
            let path = crate::settings::user_preferences_toml_file_path();
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&path, "");
            }
        })
        .with_action(|app, window_id, _data| {
            dispatch_workspace_action(app, window_id, &WorkspaceAction::OpenSettingsFile);
        })
}
