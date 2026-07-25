//! SFTP browser view UI unit tests
//!
//! Verify view state management and action-handling logic. Uses App::test() + the mock platform,
//! with no real SSH connection (the view starts in the Disconnected state).
//! author: logic
//! date: 2026-05-27

use std::path::PathBuf;

use warp_core::ui::appearance::Appearance;
use warpui::platform::WindowStyle;
use warpui::TypedActionView;

use crate::settings_view::keybindings::KeybindingChangedNotifier;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspaces::user_workspaces::UserWorkspaces;

use pathfinder_geometry::vector::Vector2F;

use super::browser::{SftpBrowserAction, SftpBrowserView};
use super::types::{ConnectionState, Dialog, TransferDirection, TransferState};
use crate::editor::EditorView;

/// Initialize the minimal set of singletons the tests need
fn initialize_app(app: &mut warpui::App) {
    use crate::workspace::ToastStack;

    initialize_settings_for_tests(app);
    app.add_singleton_model(|_| Appearance::mock());
    app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
    app.add_singleton_model(|_| ToastStack);
    // Registered because the window render path (root_view) reads the
    // UserWorkspaces singleton; without it every windowed test panics with
    // "singleton model ... never registered".
    app.add_singleton_model(UserWorkspaces::default_mock);

    // The SSH manager needs a SQLite path; use a temp file so query failures don't panic
    let temp_db = std::env::temp_dir().join("warp_sftp_test.sqlite");
    let _ = warp_ssh_manager::set_database_path(temp_db);
}

/// Create a SftpBrowserView and place it in a window
///
/// The view starts Disconnected (no SSH connection), which doesn't affect the UI-state logic tests.
fn create_view(app: &mut warpui::App) -> (warpui::WindowId, warpui::ViewHandle<SftpBrowserView>) {
    app.add_window(WindowStyle::NotStealFocus, |ctx| {
        SftpBrowserView::new("test-node".to_string(), ctx)
    })
}

// ============================================================
// Drag state tests
// ============================================================

/// Verify DragFilesEnter sets is_drag_hovering to true
#[test]
fn test_drag_files_enter() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
        });

        view.read(&app, |view, _| {
            assert!(
                view.is_drag_hovering,
                "is_drag_hovering should be true after DragFilesEnter"
            );
        });
    });
}

/// Verify DragFilesLeave sets is_drag_hovering to false
#[test]
fn test_drag_files_leave() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // First enter the hover state
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
        });
        // Then leave
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesLeave, ctx);
        });

        view.read(&app, |view, _| {
            assert!(
                !view.is_drag_hovering,
                "is_drag_hovering should be false after DragFilesLeave"
            );
        });
    });
}

/// Verify DragAndDropFiles resets is_drag_hovering
#[test]
fn test_drag_and_drop_resets_hover() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // First enter hover
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
        });
        // Drop files (no SFTP connection, so the transfer fails but doesn't crash)
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::DragAndDropFiles(vec![PathBuf::from("/tmp/test.txt")]),
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert!(
                !view.is_drag_hovering,
                "is_drag_hovering should reset to false after DragAndDropFiles"
            );
        });
    });
}

// ============================================================
// Selection state tests
// ============================================================

/// Verify SelectEntry selects an entry
#[test]
fn test_select_entry() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(0), ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.selected.contains(&0), "index 0 should be selected after SelectEntry(0)");
        });
    });
}

/// Verify SelectEntry toggles selection (single-select mode: re-selecting the same item keeps it selected)
#[test]
fn test_toggle_select_entry() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // Select index 2
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(2), ctx);
        });
        view.read(&app, |view, _| {
            assert!(view.selected.contains(&2));
        });

        // Select index 5 -> clears the previous, keeps only 5
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(5), ctx);
        });
        view.read(&app, |view, _| {
            assert!(!view.selected.contains(&2), "index 2 should be deselected after SelectEntry(5)");
            assert!(view.selected.contains(&5), "index 5 should be selected after SelectEntry(5)");
        });
    });
}

// ============================================================
// Search filter tests
// ============================================================

/// Verify SetSearchFilter sets the search text
#[test]
fn test_set_search_filter() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SetSearchFilter("txt".to_string()), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.search_filter.as_deref(), Some("txt"));
        });
    });
}

/// Verify ClearSearchFilter clears the search text
#[test]
fn test_clear_search_filter() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // First set it
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SetSearchFilter("log".to_string()), ctx);
        });
        // Then clear it
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::ClearSearchFilter, ctx);
        });

        view.read(&app, |view, _| {
            assert!(
                view.search_filter.is_none(),
                "should be None after ClearSearchFilter"
            );
        });
    });
}

// ============================================================
// Navigation tests
// ============================================================

/// Verify NavigateUp at the root directory doesn't change the path
#[test]
fn test_navigate_up_from_root() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/"));
        });

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::NavigateUp, ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(
                view.current_path,
                PathBuf::from("/"),
                "NavigateUp at the root should stay unchanged"
            );
        });
    });
}

// ============================================================
// Initial state tests
// ============================================================

/// Verify the view's initial state is correct
#[test]
fn test_initial_state() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.read(&app, |view, _| {
            assert!(view.entries.is_empty(), "the initial entry list should be empty");
            assert!(view.selected.is_empty(), "the initial selection set should be empty");
            assert!(view.transfers.is_empty(), "the initial transfer list should be empty");
            assert!(view.search_filter.is_none(), "the initial search filter should be None");
            assert!(!view.is_drag_hovering, "initial drag-hover should be false");
        });
    });
}

// ============================================================
// Context menu tests
// ============================================================

/// Verify the ContextMenu action sets the context_menu state and selects the entry
#[test]
fn test_context_menu_sets_state() {
    use pathfinder_geometry::vector::Vector2F;

    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        let position = Vector2F::new(100.0, 200.0);
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 3,
                    position,
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert!(
                view.context_menu.is_some(),
                "context_menu should be Some after ContextMenu"
            );
            let cm = view.context_menu.as_ref().unwrap();
            assert_eq!(cm.entry_index, 3, "entry_index should be 3");
            assert_eq!(cm.position, position, "position should match the passed-in value");
            assert!(
                view.selected.contains(&3),
                "index 3 should be selected after ContextMenu"
            );
        });
    });
}

/// Verify CloseContextMenu clears the context_menu state
#[test]
fn test_close_context_menu_clears_state() {
    use pathfinder_geometry::vector::Vector2F;

    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // First open the context menu
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 1,
                    position: Vector2F::new(50.0, 50.0),
                },
                ctx,
            );
        });
        view.read(&app, |view, _| {
            assert!(view.context_menu.is_some(), "the menu should be open");
        });

        // Close the menu
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::CloseContextMenu, ctx);
        });

        view.read(&app, |view, _| {
            assert!(
                view.context_menu.is_none(),
                "context_menu should be None after CloseContextMenu"
            );
        });
    });
}

/// Verify ContextMenu replaces the previous menu state
#[test]
fn test_context_menu_replaces_previous() {
    use pathfinder_geometry::vector::Vector2F;

    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // Open the first menu
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 0,
                    position: Vector2F::new(10.0, 10.0),
                },
                ctx,
            );
        });

        // Open the second menu (different position and index)
        let new_position = Vector2F::new(300.0, 400.0);
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 5,
                    position: new_position,
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            let cm = view.context_menu.as_ref().unwrap();
            assert_eq!(cm.entry_index, 5, "should update to the new entry_index");
            assert_eq!(
                cm.position, new_position,
                "should update to the new position"
            );
            assert!(
                view.selected.contains(&5),
                "the new index 5 should be selected"
            );
            assert!(
                !view.selected.contains(&0),
                "the old index 0 should be deselected"
            );
        });
    });
}

// ============================================================
// Context menu boundary-condition tests
// ============================================================

/// Verify ContextMenu handles index=0 correctly
#[test]
fn test_context_menu_zero_index() {
    use pathfinder_geometry::vector::Vector2F;

    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        let position = Vector2F::new(0.0, 0.0);
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 0,
                    position,
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            let cm = view.context_menu.as_ref().unwrap();
            assert_eq!(cm.entry_index, 0, "index=0 should be stored correctly");
            assert_eq!(cm.position, position, "position should be stored correctly");
            assert!(view.selected.contains(&0), "index 0 should be selected");
        });
    });
}

/// Verify ContextMenu with a large index value doesn't panic
#[test]
fn test_context_menu_large_index() {
    use pathfinder_geometry::vector::Vector2F;

    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        let position = Vector2F::new(500.0, 600.0);
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 999,
                    position,
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            let cm = view.context_menu.as_ref().unwrap();
            assert_eq!(cm.entry_index, 999, "a large index should be stored correctly");
            assert!(view.selected.contains(&999), "the large index should be selected");
        });
    });
}

/// Verify ContextMenu handles negative coordinates correctly
#[test]
fn test_context_menu_negative_position() {
    use pathfinder_geometry::vector::Vector2F;

    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        let position = Vector2F::new(-50.0, -100.0);
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 1,
                    position,
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            let cm = view.context_menu.as_ref().unwrap();
            assert_eq!(cm.position, position, "negative coordinates should be stored correctly");
        });
    });
}

/// Verify CloseContextMenu doesn't panic when no menu is open
#[test]
fn test_close_context_menu_when_none() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // No menu in the initial state
        view.read(&app, |view, _| {
            assert!(view.context_menu.is_none(), "there should be no menu initially");
        });

        // Closing directly should not panic
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::CloseContextMenu, ctx);
        });

        view.read(&app, |view, _| {
            assert!(
                view.context_menu.is_none(),
                "should still be None after closing"
            );
        });
    });
}

/// Verify ContextMenu clears the previous selection and selects the new entry
#[test]
fn test_context_menu_clears_previous_selection() {
    use pathfinder_geometry::vector::Vector2F;

    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // First select entries 2 and 3 (via two SelectEntry calls)
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(2), ctx);
        });
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(3), ctx);
        });
        view.read(&app, |view, _| {
            assert!(view.selected.contains(&3), "3 should be selected");
            assert!(!view.selected.contains(&2), "single-select mode should clear 2");
        });

        // Right-click entry 7
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 7,
                    position: Vector2F::new(200.0, 300.0),
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert!(view.selected.contains(&7), "7 should be selected");
            assert!(!view.selected.contains(&3), "the old selection 3 should be cleared");
            assert_eq!(view.selected.len(), 1, "there should be only one selected item");
        });
    });
}

/// Verify opening and closing the menu repeatedly doesn't leak state
#[test]
fn test_context_menu_multiple_open_close_cycles() {
    use pathfinder_geometry::vector::Vector2F;

    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        for i in 0..5 {
            // Open the menu
            view.update(&mut app, |view, ctx| {
                view.handle_action(
                    &SftpBrowserAction::ContextMenu {
                        index: i,
                        position: Vector2F::new(i as f32 * 10.0, i as f32 * 20.0),
                    },
                    ctx,
                );
            });
            view.read(&app, |view, _| {
                assert!(view.context_menu.is_some(), "open #{i} should succeed");
            });

            // Close the menu
            view.update(&mut app, |view, ctx| {
                view.handle_action(&SftpBrowserAction::CloseContextMenu, ctx);
            });
            view.read(&app, |view, _| {
                assert!(
                    view.context_menu.is_none(),
                    "should be None after close #{i}"
                );
            });
        }
    });
}

// ============================================================
// Menu item action tests
// ============================================================

/// Verify the SftpBrowserAction::DetailsEntry variant constructs correctly
#[test]
fn test_action_details_entry() {
    let action = SftpBrowserAction::DetailsEntry(42);
    assert!(matches!(action, SftpBrowserAction::DetailsEntry(42)));
}

/// Verify the SftpBrowserAction::DeleteEntry variant constructs correctly
#[test]
fn test_action_delete_entry() {
    let action = SftpBrowserAction::DeleteEntry(10);
    assert!(matches!(action, SftpBrowserAction::DeleteEntry(10)));
}

/// Verify the SftpBrowserAction::RenameEntry variant constructs correctly
#[test]
fn test_action_rename_entry() {
    let action = SftpBrowserAction::RenameEntry(5);
    assert!(matches!(action, SftpBrowserAction::RenameEntry(5)));
}

/// Verify the SftpBrowserAction::DownloadEntry variant constructs correctly
#[test]
fn test_action_download_entry() {
    let action = SftpBrowserAction::DownloadEntry(3);
    assert!(matches!(action, SftpBrowserAction::DownloadEntry(3)));
}

/// Verify the SftpBrowserAction::OpenEntry variant constructs correctly
#[test]
fn test_action_open_entry() {
    let action = SftpBrowserAction::OpenEntry(1);
    assert!(matches!(action, SftpBrowserAction::OpenEntry(1)));
}

/// Verify the SftpBrowserAction::ContextMenu variant constructs correctly
#[test]
fn test_action_context_menu_variant() {
    use pathfinder_geometry::vector::Vector2F;
    let action = SftpBrowserAction::ContextMenu {
        index: 3,
        position: Vector2F::new(100.0, 200.0),
    };
    assert!(matches!(
        action,
        SftpBrowserAction::ContextMenu {
            index: 3,
            ..
        }
    ));
}

/// Verify the SftpBrowserAction::CloseContextMenu variant constructs correctly
#[test]
fn test_action_close_context_menu_variant() {
    let action = SftpBrowserAction::CloseContextMenu;
    assert!(matches!(action, SftpBrowserAction::CloseContextMenu));
}

// ============================================================
// DeleteEntry action-handling tests
// ============================================================

/// Verify DeleteEntry doesn't panic with no SFTP connection
#[test]
fn test_delete_entry_no_connection() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // Executing DeleteEntry with no SFTP connection should not panic
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DeleteEntry(0), ctx);
        });
    });
}

/// Verify RenameEntry doesn't panic with no SFTP connection
#[test]
fn test_rename_entry_no_connection() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::RenameEntry(0), ctx);
        });
    });
}

// ============================================================
// Category 1: dialog-operation safety with no connection
// ============================================================

/// Verify ConfirmDelete doesn't panic with no dialog and no connection
#[test]
fn test_confirm_delete_no_connection_no_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::ConfirmDelete, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify ConfirmDelete handles safely when a dialog exists but there's no connection
#[test]
fn test_confirm_delete_no_connection_with_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.dialog = Some(Dialog::DeleteConfirm {
                paths: vec![PathBuf::from("/tmp/test")],
                is_dirs: vec![false],
            });
            view.handle_action(&SftpBrowserAction::ConfirmDelete, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify ConfirmRename doesn't panic with no dialog and no connection
#[test]
fn test_confirm_rename_no_connection_no_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::ConfirmRename, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify ConfirmRename shows an error and closes the dialog when a dialog exists but there's no connection
#[test]
fn test_confirm_rename_no_connection_with_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.dialog = Some(Dialog::Rename {
                path: PathBuf::from("/home/old.txt"),
                original_name: "old.txt".to_string(),
            });
            // First enter a non-empty name to skip the empty-name check
            view.rename_editor.update(ctx, |e: &mut EditorView, ctx| {
                e.set_buffer_text("new_name", ctx);
            });
            view.handle_action(&SftpBrowserAction::ConfirmRename, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify ConfirmNewFolder doesn't panic with no dialog and no connection
#[test]
fn test_confirm_new_folder_no_connection_no_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::ConfirmNewFolder, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify ConfirmNewFolder shows an error and closes the dialog when a dialog exists but there's no connection
#[test]
fn test_confirm_new_folder_no_connection_with_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.dialog = Some(Dialog::CreateFolder {
                parent_path: PathBuf::from("/home"),
            });
            // First enter a non-empty name to skip the empty-name check
            view.new_folder_editor.update(ctx, |e: &mut EditorView, ctx| {
                e.set_buffer_text("new_folder", ctx);
            });
            view.handle_action(&SftpBrowserAction::ConfirmNewFolder, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify ConfirmMove doesn't panic with no dialog and no connection
#[test]
fn test_confirm_move_no_connection_no_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::ConfirmMove, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify ConfirmMove shows an error and closes the dialog when a dialog exists but there's no connection
#[test]
fn test_confirm_move_no_connection_with_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.dialog = Some(Dialog::Move {
                source: PathBuf::from("/home/file.txt"),
                target_dir: PathBuf::from("/home/backup"),
            });
            view.handle_action(&SftpBrowserAction::ConfirmMove, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

// ============================================================
// Category 2: navigation boundary tests
// ============================================================

/// Verify NavigateTo to the current path doesn't create duplicate history
#[test]
fn test_navigate_to_same_path() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::NavigateTo(PathBuf::from("/")), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/"));
            assert_eq!(view.path_history.len(), 1);
        });
    });
}

/// Verify NavigateTo updates correctly for a deep path
#[test]
fn test_navigate_to_deep_path() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::NavigateTo(PathBuf::from("/a/b/c/d")),
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/a/b/c/d"));
            assert_eq!(view.path_history.len(), 2);
        });
    });
}

/// Verify NavigateTo normalizes backslashes to forward slashes
#[test]
fn test_navigate_to_backslash_path() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::NavigateTo(PathBuf::from(r"home\user")),
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("home/user"));
        });
    });
}

/// Verify GoBack is a no-op at the initial history position
#[test]
fn test_go_back_at_initial() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::GoBack, ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/"));
        });
    });
}

/// Verify GoForward is a no-op at the initial history position
#[test]
fn test_go_forward_at_initial() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::GoForward, ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/"));
        });
    });
}

/// Verify GoUp is a no-op from the root path
#[test]
fn test_go_up_from_root_via_action() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::GoUp, ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/"));
        });
    });
}

/// Verify GoBack/GoForward history tracking is correct after multi-step navigation
#[test]
fn test_multiple_navigate_then_back_forward() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::NavigateTo(PathBuf::from("/home")), ctx);
            view.handle_action(&SftpBrowserAction::NavigateTo(PathBuf::from("/var")), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/var"));
            assert_eq!(view.path_history.len(), 3);
            assert_eq!(view.history_index, 2);
        });

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::GoBack, ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/home"));
            assert_eq!(view.history_index, 1);
        });

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::GoForward, ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/var"));
            assert_eq!(view.history_index, 2);
        });
    });
}

// ============================================================
// Category 3: dialog open/close cycle tests
// ============================================================

/// Verify NewFolder opens the CreateFolder dialog
#[test]
fn test_new_folder_opens_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::NewFolder, ctx);
        });

        view.read(&app, |view, _| {
            assert!(matches!(view.dialog, Some(Dialog::CreateFolder { .. })));
        });
    });
}

/// Verify CloseDialog clears the dialog
#[test]
fn test_close_dialog_clears() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::NewFolder, ctx);
            view.handle_action(&SftpBrowserAction::CloseDialog, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify ConfirmOverwrite closes the dialog
#[test]
fn test_confirm_overwrite_closes_dialog() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.dialog = Some(Dialog::OverwriteConfirm {
                source: PathBuf::from("/a"),
                target: PathBuf::from("/b"),
                file_size: 0,
                direction: TransferDirection::Download,
            });
            view.handle_action(&SftpBrowserAction::ConfirmOverwrite, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify CloseDialog doesn't panic when there's no dialog
#[test]
fn test_close_dialog_when_none() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::CloseDialog, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify stability across repeated dialog open/close cycles
#[test]
fn test_dialog_multiple_cycles() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        for _ in 0..3 {
            view.update(&mut app, |view, ctx| {
                view.handle_action(&SftpBrowserAction::NewFolder, ctx);
            });
            view.read(&app, |view, _| {
                assert!(view.dialog.is_some());
            });

            view.update(&mut app, |view, ctx| {
                view.handle_action(&SftpBrowserAction::CloseDialog, ctx);
            });
            view.read(&app, |view, _| {
                assert!(view.dialog.is_none());
            });
        }
    });
}

// ============================================================
// Category 4: transfer-task lifecycle tests
// ============================================================

/// Verify cancelling a nonexistent task ID doesn't panic
#[test]
fn test_cancel_transfer_nonexistent_id() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::CancelTransfer(999), ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.transfers.is_empty());
        });
    });
}

/// Verify cancelling a nonexistent task with ID 0 doesn't panic
#[test]
fn test_cancel_transfer_zero_id() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::CancelTransfer(0), ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.transfers.is_empty());
        });
    });
}

/// Verify DownloadSaveAs with an out-of-bounds index doesn't panic and creates no orphan task
#[test]
fn test_download_save_as_out_of_range() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::DownloadSaveAs {
                    index: 100,
                    local_path: "/tmp/out.txt".to_string(),
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert!(view.transfers.is_empty());
            assert_eq!(view.next_transfer_id, 1);
        });
    });
}

/// Verify DownloadSaveAs doesn't panic with index=0 on an empty entry list
#[test]
fn test_download_save_as_zero_index_empty() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::DownloadSaveAs {
                    index: 0,
                    local_path: "/tmp/out.txt".to_string(),
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert!(view.transfers.is_empty());
        });
    });
}

/// Verify ExecuteUpload marks the task Failed for a nonexistent local file with no connection
#[test]
fn test_execute_upload_nonexistent_file() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ExecuteUpload("/no/such/file.txt".to_string()),
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert_eq!(view.transfers.len(), 1);
            assert!(matches!(
                view.transfers[0].state,
                TransferState::Failed(_)
            ));
        });
    });
}

// ============================================================
// Category 5: DetailsEntry boundary tests
// ============================================================

/// Verify DetailsEntry with an out-of-bounds index doesn't panic
#[test]
fn test_details_entry_out_of_range() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DetailsEntry(999), ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify DetailsEntry doesn't panic with index=0 on empty entries
#[test]
fn test_details_entry_zero_empty() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DetailsEntry(0), ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify DetailsEntry with an extremely large index doesn't panic
#[test]
fn test_details_entry_usize_max() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DetailsEntry(usize::MAX), ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

// ============================================================
// Category 6: OpenEntry / DownloadEntry no-entry tests
// ============================================================

/// Verify OpenEntry with an out-of-bounds index doesn't panic and leaves the path unchanged
#[test]
fn test_open_entry_out_of_range() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::OpenEntry(999), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/"));
        });
    });
}

/// Verify OpenEntry doesn't panic with index=0 on empty entries
#[test]
fn test_open_entry_zero_empty() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::OpenEntry(0), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/"));
        });
    });
}

/// Verify DownloadEntry doesn't panic on an empty entry list
#[test]
fn test_download_entry_empty_entries() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DownloadEntry(0), ctx);
        });
        // Passing = it doesn't panic
    });
}

// ============================================================
// Category 7: selection and deletion boundary tests
// ============================================================

/// Verify DeleteSelected doesn't panic with an empty selection set
#[test]
fn test_delete_selected_empty_selection() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DeleteSelected, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify DeleteSelected doesn't panic with a selection but no entries
#[test]
fn test_delete_selected_no_entries() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(0), ctx);
            view.handle_action(&SftpBrowserAction::DeleteSelected, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify SelectEntry accepts usize::MAX without panicking
#[test]
fn test_select_entry_usize_max() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(usize::MAX), ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.selected.contains(&usize::MAX));
        });
    });
}

/// Verify each SelectEntry clears the previous selection
#[test]
fn test_multiple_select_clears_previous() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(1), ctx);
            view.handle_action(&SftpBrowserAction::SelectEntry(3), ctx);
            view.handle_action(&SftpBrowserAction::SelectEntry(7), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.selected.len(), 1);
            assert!(view.selected.contains(&7));
            assert!(!view.selected.contains(&1));
            assert!(!view.selected.contains(&3));
        });
    });
}

// ============================================================
// Category 8: render safety tests
// ============================================================

/// Verify initial-state consistency (the constructor attempts to connect, so it may be Failed or Disconnected)
#[test]
fn test_render_disconnected_state() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.read(&app, |view, _| {
            // The constructor calls connect_to_server; with no SSH service in the test env, the state is Failed
            assert!(matches!(
                view.connection,
                ConnectionState::Failed(_) | ConnectionState::Disconnected
            ));
            assert!(!view.is_loading);
            assert!(view.entries.is_empty());
            assert!(view.dialog.is_none());
            assert!(view.context_menu.is_none());
        });
    });
}

/// Verify the drag-hover state is set correctly
#[test]
fn test_render_with_drag_hover() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.is_drag_hovering);
        });
    });
}

/// Verify the search-filter state is set correctly
#[test]
fn test_render_with_search_filter() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SetSearchFilter("test".to_string()), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.search_filter.as_deref(), Some("test"));
        });
    });
}

/// Verify the context-menu state is set correctly
#[test]
fn test_render_with_context_menu() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 0,
                    position: Vector2F::new(10.0, 20.0),
                },
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert!(view.context_menu.is_some());
        });
    });
}

/// Verify the dialog-open state is set correctly
#[test]
fn test_render_with_dialog_open() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::NewFolder, ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.dialog.is_some());
        });
    });
}

/// Verify the state is correct after a transfer task is created
#[test]
fn test_render_with_transfer_task() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::ExecuteUpload("/tmp/x.txt".to_string()),
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert_eq!(view.transfers.len(), 1);
        });
    });
}

/// Verify no panic when all overlays exist simultaneously
#[test]
fn test_render_all_overlays_combined() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
            view.handle_action(&SftpBrowserAction::SetSearchFilter("x".to_string()), ctx);
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 0,
                    position: Vector2F::new(5.0, 5.0),
                },
                ctx,
            );
            view.handle_action(&SftpBrowserAction::NewFolder, ctx);
            view.handle_action(
                &SftpBrowserAction::ExecuteUpload("/tmp/test.txt".to_string()),
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert!(view.is_drag_hovering);
            assert!(view.search_filter.is_some());
            assert!(view.context_menu.is_some());
            assert!(view.dialog.is_some());
            assert_eq!(view.transfers.len(), 1);
        });
    });
}

/// Verify state is correctly cleared after all overlays are closed
#[test]
fn test_render_after_close_all_overlays() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // Open all overlays
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
            view.handle_action(&SftpBrowserAction::SetSearchFilter("x".to_string()), ctx);
            view.handle_action(
                &SftpBrowserAction::ContextMenu {
                    index: 0,
                    position: Vector2F::new(5.0, 5.0),
                },
                ctx,
            );
            view.handle_action(&SftpBrowserAction::NewFolder, ctx);
        });

        // Close all overlays
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesLeave, ctx);
            view.handle_action(&SftpBrowserAction::ClearSearchFilter, ctx);
            view.handle_action(&SftpBrowserAction::CloseContextMenu, ctx);
            view.handle_action(&SftpBrowserAction::CloseDialog, ctx);
        });

        view.read(&app, |view, _| {
            assert!(!view.is_drag_hovering);
            assert!(view.search_filter.is_none());
            assert!(view.context_menu.is_none());
            assert!(view.dialog.is_none());
        });
    });
}

/// Verify the initial path-history state
#[test]
fn test_render_path_history_initial() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.read(&app, |view, _| {
            assert_eq!(view.path_history, vec![PathBuf::from("/")]);
            assert_eq!(view.history_index, 0);
        });
    });
}

/// Verify the initial is_loading is false
#[test]
fn test_render_is_loading_initial_false() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.read(&app, |view, _| {
            assert!(!view.is_loading);
        });
    });
}
