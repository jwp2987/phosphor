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

/// Turns `FeatureFlag::GroupedTabs` on for the rest of the test, and checks the
/// one precondition that forcing the flag cannot fake: that this build ships
/// tab groups at all.
///
/// The action is load-bearing setup. Every grouping path is gated on
/// `FeatureFlag::GroupedTabs.is_enabled()` at run time — the model mutations
/// (`workspace/view.rs:11753`, `:11807`, `:11841`, `:11872`), `tab_bar_slots`
/// (`:25010`) and the insertion-group resolution `on_tab_drag` consults
/// (`:24799`) — so a tab-group test that merely assumed the flag would degrade
/// into a test of ungrouped tabs that still passes. `set_user_preference` is
/// the process-global tier `is_enabled` consults ahead of the compiled-in state
/// (unlike `override_enabled`, which is thread-local and would not hold for the
/// app's UI thread).
///
/// The assertion used to be `is_enabled()` alone, which could not fail: the
/// action had just written the tier `is_enabled` reads first
/// (`warp_features/src/lib.rs:972-974`, user preference ahead of
/// `FLAG_STATES`). The `cfg!` half is the part with teeth. `grouped_tabs` is a
/// default cargo feature (`app/Cargo.toml:655`) that gates the flag's
/// default-on entry (`app/src/lib.rs:3283`) and the settings toggle
/// (`settings_view/appearance_page.rs:1588`), and `settings/init.rs:301`
/// samples the flag *before* any preference is written precisely so the toggle
/// "can turn tab groups off but never on in a build that did not have them"
/// (`settings/init.rs:427-429`). `set_user_preference` walks straight past that
/// guard, so without this check a build compiled without the feature would run
/// the whole tab-group suite against a configuration it does not ship, and
/// report PASS.
///
/// The `is_enabled()` half is kept because it is not inert: it still fails if
/// `GroupedTabs` is added to `FORCE_DISABLED_FLAGS` (consulted ahead of every
/// tier, `warp_features/src/lib.rs:959-961`) or if a thread-local
/// `override_enabled(false)` is left in place (consulted ahead of the user
/// preference). It is not evidence that grouping is live, and does not claim to
/// be — that comes from the next step, whose `group_id_for_anchor` panics if
/// `create_tab_group_from_tab` produced no group.
pub fn ensure_grouped_tabs_enabled() -> TestStep {
    TestStep::new("Ensure grouped tabs are enabled")
        .with_action(|_app, _window_id, _data| {
            FeatureFlag::GroupedTabs.set_user_preference(true);
        })
        .add_named_assertion(
            "this build ships tab groups, and GroupedTabs is enabled",
            |_app, _window_id| {
                async_assert!(
                    cfg!(feature = "grouped_tabs") && FeatureFlag::GroupedTabs.is_enabled(),
                    "tab-group tests require a build with the `grouped_tabs` cargo feature: \
                     forcing FeatureFlag::GroupedTabs on without it exercises a configuration \
                     this build does not ship"
                )
            },
        )
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
