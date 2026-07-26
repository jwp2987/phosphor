//! Right-click context menu rendering component
//!
//! Renders the right-click menu for a file entry, including open, download, rename, delete, and details actions.
//! author: logic
//! date: 2026-05-26

use pathfinder_geometry::vector::Vector2F;
use warp_core::ui::appearance::Appearance;
use warpui::elements::{
    Border, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Dismiss, Flex, Hoverable,
    MainAxisSize, ParentElement, Radius, SavePosition, Text,
};
use warpui::platform::Cursor;
use warpui::Element;

/// Context menu width
const CONTEXT_MENU_WIDTH: f32 = 150.0;

use crate::sftp_manager::browser::SftpBrowserAction;

/// Context menu state
#[derive(Debug)]
pub struct ContextMenuState {
    /// Index of the associated file entry
    pub entry_index: usize,
    /// Menu popup position
    pub position: Vector2F,
}

impl ContextMenuState {
    /// Create a new context menu state
    pub fn new(entry_index: usize, position: Vector2F) -> Self {
        Self { entry_index, position }
    }
}

/// Menu item definition
struct MenuItem {
    /// Display label
    label: String,
    /// Associated action
    action: SftpBrowserAction,
}

/// Build the list of file right-click menu items
fn build_file_menu_items(entry_index: usize) -> Vec<MenuItem> {
    vec![
        MenuItem {
            label: String::from("Open"),
            action: SftpBrowserAction::OpenEntry(entry_index),
        },
        MenuItem {
            label: String::from("Download"),
            action: SftpBrowserAction::DownloadEntry(entry_index),
        },
        MenuItem {
            label: String::from("Rename"),
            action: SftpBrowserAction::RenameEntry(entry_index),
        },
        MenuItem {
            label: String::from("Delete"),
            action: SftpBrowserAction::DeleteEntry(entry_index),
        },
        MenuItem {
            label: String::from("Details"),
            action: SftpBrowserAction::DetailsEntry(entry_index),
        },
    ]
}

/// Render a single menu item
fn render_menu_item(
    label: &str,
    action: SftpBrowserAction,
    appearance: &Appearance,
    position_id: &str,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let hover_bg = theme.surface_3();
    let default_bg = theme.surface_2();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();
    let label_owned = label.to_string();

    let item_el = Hoverable::new(Default::default(), move |state| {
        let bg = if state.is_hovered() || state.is_clicked() {
            hover_bg
        } else {
            default_bg
        };
        let text_el = Text::new_inline(label_owned.clone(), ui_font, ui_font_size)
            .with_color(text_color.into())
            .finish();
        Container::new(text_el)
            .with_background(bg)
            .with_padding_left(12.0)
            .with_padding_right(12.0)
            .with_padding_top(6.0)
            .with_padding_bottom(6.0)
            .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_mouse_down(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
        ctx.dispatch_typed_action(SftpBrowserAction::CloseContextMenu);
    })
    .finish();

    SavePosition::new(item_el, position_id).finish()
}

/// Render the right-click context menu
pub fn render_context_menu(state: &ContextMenuState, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let menu_items = build_file_menu_items(state.entry_index);

    let mut col = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_main_axis_size(MainAxisSize::Min);

    for item in &menu_items {
        let position_id = match &item.action {
            SftpBrowserAction::OpenEntry(_) => "sftp_ctx:open",
            SftpBrowserAction::DownloadEntry(_) => "sftp_ctx:download",
            SftpBrowserAction::RenameEntry(_) => "sftp_ctx:rename",
            SftpBrowserAction::DeleteEntry(_) => "sftp_ctx:delete",
            SftpBrowserAction::DetailsEntry(_) => "sftp_ctx:details",
            SftpBrowserAction::NavigateTo(_)
            | SftpBrowserAction::GoUp
            | SftpBrowserAction::GoBack
            | SftpBrowserAction::GoForward
            | SftpBrowserAction::Refresh
            | SftpBrowserAction::SelectEntry(_)
            | SftpBrowserAction::UploadFile
            | SftpBrowserAction::NewFolder
            | SftpBrowserAction::ConfirmDelete
            | SftpBrowserAction::ConfirmRename
            | SftpBrowserAction::ConfirmNewFolder
            | SftpBrowserAction::ConfirmOverwrite
            | SftpBrowserAction::ContextMenu { .. }
            | SftpBrowserAction::CloseContextMenu
            | SftpBrowserAction::CloseDialog
            | SftpBrowserAction::SetSearchFilter(_)
            | SftpBrowserAction::ClearSearchFilter
            | SftpBrowserAction::NavigateUp
            | SftpBrowserAction::DeleteSelected
            | SftpBrowserAction::CreateFolder
            | SftpBrowserAction::DragFilesEnter
            | SftpBrowserAction::DragFilesLeave
            | SftpBrowserAction::DragAndDropFiles(_)
            | SftpBrowserAction::ExecuteUpload(_)
            | SftpBrowserAction::DownloadSaveAs { .. }
            | SftpBrowserAction::ConfirmMove
            | SftpBrowserAction::CancelTransfer(_)
            | SftpBrowserAction::ToggleTransferPanel
            | SftpBrowserAction::ConfirmCloseTransferPanel => "sftp_ctx:unknown",
        };
        let el = render_menu_item(&item.label, item.action.clone(), appearance, position_id);
        col.add_child(el);
    }

    let menu_inner = ConstrainedBox::new(
        Container::new(col.finish())
            .with_background(theme.surface_2())
            .with_border(Border::all(1.0).with_border_color(theme.surface_3().into()))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.0)))
            .with_uniform_padding(4.0)
            .finish(),
    )
    .with_width(CONTEXT_MENU_WIDTH)
    .finish();

    Dismiss::new(menu_inner)
        .prevent_interaction_with_other_elements()
        .on_dismiss(|ctx, _| {
            ctx.dispatch_typed_action(SftpBrowserAction::CloseContextMenu);
        })
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pathfinder_geometry::vector::Vector2F;
    use warpui::TypedActionView;

    // ============================================================
    // ContextMenuState tests
    // ============================================================

    /// Test that the ContextMenuState constructor sets fields correctly
    #[test]
    fn test_context_menu_state_new() {
        let position = Vector2F::new(100.0, 200.0);
        let state = ContextMenuState::new(3, position);
        assert_eq!(state.entry_index, 3);
        assert_eq!(state.position, position);
    }

    /// Test that ContextMenuState constructs correctly when index=0
    #[test]
    fn test_context_menu_state_zero_index() {
        let position = Vector2F::new(0.0, 0.0);
        let state = ContextMenuState::new(0, position);
        assert_eq!(state.entry_index, 0);
        assert_eq!(state.position, position);
    }

    /// Test ContextMenuState with a large index value
    #[test]
    fn test_context_menu_state_large_index() {
        let position = Vector2F::new(500.0, 600.0);
        let state = ContextMenuState::new(usize::MAX, position);
        assert_eq!(state.entry_index, usize::MAX);
    }

    /// Test ContextMenuState with negative coordinates (Vector2F supports negatives)
    #[test]
    fn test_context_menu_state_negative_position() {
        let position = Vector2F::new(-50.0, -100.0);
        let state = ContextMenuState::new(1, position);
        assert_eq!(state.position, position);
    }

    // ============================================================
    // build_file_menu_items tests
    // ============================================================

    /// Test that there are 5 menu items
    #[test]
    fn test_build_file_menu_items_count() {
        let items = build_file_menu_items(0);
        assert_eq!(items.len(), 5, "should have 5 menu items");
    }

    /// Test that the menu item labels are correct
    #[test]
    fn test_build_file_menu_items_labels() {
        let items = build_file_menu_items(0);
        let expected_labels = ["Open", "Download", "Rename", "Delete", "Details"];
        for (item, expected) in items.iter().zip(expected_labels.iter()) {
            assert_eq!(&item.label.as_str(), expected, "label should be {}", expected);
        }
    }

    /// Test that menu item actions bind the correct index
    #[test]
    fn test_build_file_menu_items_actions_index() {
        let index = 7;
        let items = build_file_menu_items(index);

        assert!(matches!(&items[0].action, SftpBrowserAction::OpenEntry(7)));
        assert!(matches!(&items[1].action, SftpBrowserAction::DownloadEntry(7)));
        assert!(matches!(&items[2].action, SftpBrowserAction::RenameEntry(7)));
        assert!(matches!(&items[3].action, SftpBrowserAction::DeleteEntry(7)));
        assert!(matches!(&items[4].action, SftpBrowserAction::DetailsEntry(7)));
    }

    /// Test that menu item actions are correct when index=0
    #[test]
    fn test_build_file_menu_items_zero_index() {
        let items = build_file_menu_items(0);
        assert!(matches!(&items[0].action, SftpBrowserAction::OpenEntry(0)));
        assert!(matches!(&items[3].action, SftpBrowserAction::DeleteEntry(0)));
        assert!(matches!(&items[4].action, SftpBrowserAction::DetailsEntry(0)));
    }

    // ============================================================
    // render_context_menu render test (verified through the browser view)
    // ============================================================

    /// Rendering after triggering ContextMenu through the browser view should not panic
    #[test]
    fn test_render_context_menu_via_browser() {
        use crate::settings_view::keybindings::KeybindingChangedNotifier;
        use crate::test_util::settings::initialize_settings_for_tests;
        use warp_core::ui::appearance::Appearance;

        warpui::App::test((), |mut app| async move {
            initialize_settings_for_tests(&mut app);
            app.add_singleton_model(|_| Appearance::mock());
            app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            // Rendering a SftpBrowserView drives the window render path
            // (root_view), which reads the UserWorkspaces singleton.
            app.add_singleton_model(crate::workspaces::user_workspaces::UserWorkspaces::default_mock);

            let temp_db = std::env::temp_dir().join("warp_sftp_ctx_test.sqlite");
            let _ = warp_ssh_manager::set_database_path(temp_db);

            let (_, view) = app.add_window(
                warpui::platform::WindowStyle::NotStealFocus,
                |ctx| crate::sftp_manager::browser::SftpBrowserView::new("test-node".to_string(), ctx),
            );

            // Trigger the context menu
            view.update(&mut app, |view, ctx| {
                view.handle_action(
                    &SftpBrowserAction::ContextMenu {
                        index: 2,
                        position: Vector2F::new(150.0, 250.0),
                    },
                    ctx,
                );
            });

            // Rendering should not panic (the view re-renders automatically)
            view.read(&app, |view, _| {
                assert!(view.context_menu.is_some(), "the menu should be open");
            });
        });
    }
}
