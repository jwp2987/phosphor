#[cfg(target_os = "macos")]
use std::fs;
use std::{path::PathBuf, sync::Arc};

use warp_core::features::FeatureFlag;
use warp_core::HostId;
use warp_util::standardized_path::StandardizedPath;

use crate::{
    app_state::{
        AppState, CodePaneSnapShot, CodePaneTabSnapshot, LeafContents, LeafSnapshot,
        NotebookPaneSnapshot, PaneNodeSnapshot, SettingsPaneSnapshot, TabGroupSnapshot,
        TabSnapshot, TerminalPaneSnapshot, WindowSnapshot,
    },
    cloud_object::{Owner, StoredObjectPermissions},
    code::buffer_location::RemotePath,
    code::editor_management::CodeSource,
    notebooks::{NotebookObject, NotebookObjectModel},
    persistence::{model, model::ObjectPermissions, schema, BlockCompleted, ModelEvent},
    server::ids::ClientId,
    server_time::ServerTimestamp,
    settings_view::SettingsSection,
    tab::SelectedTabColor,
    terminal::model::block::SerializedBlock,
    terminal::ShellLaunchData,
    themes::theme::AnsiColorIdentifier,
    workspace::tab_group::TabGroupId,
};

use super::{
    decode_path, deduplicate_events, encode_path, get_all_codebase_index_metadata,
    read_sqlite_data, save_app_state, save_codebase_index_metadata, setup_database, start_writer,
};

#[test]
fn test_deduplicate_snapshots() {
    let local_notebook = NotebookObject::new_local(
        NotebookObjectModel {
            title: "Hello".to_string(),
            data: "World".to_string(),
            ai_document_id: None,
            conversation_id: None,
        },
        Owner::mock_current_user(),
        None,
        ClientId::new(),
    );
    let completed_block_1 = BlockCompleted {
        pane_id: vec![1, 2, 3],
        block: Arc::new(SerializedBlock::default()),
        is_local: true,
    };
    let completed_block_2 = BlockCompleted {
        pane_id: vec![4, 5, 6],
        block: Arc::new(SerializedBlock::default()),
        is_local: true,
    };
    let snapshot_1 = AppState {
        active_window_index: Some(1),
        block_lists: Default::default(),
        windows: Default::default(),
        running_mcp_servers: Default::default(),
    };
    let snapshot_2 = AppState {
        active_window_index: Some(2),
        block_lists: Default::default(),
        windows: Default::default(),
        running_mcp_servers: Default::default(),
    };
    let snapshot_3 = AppState {
        active_window_index: Some(3),
        block_lists: Default::default(),
        windows: Default::default(),
        running_mcp_servers: Default::default(),
    };

    let original_events = vec![
        ModelEvent::UpsertNotebook {
            notebook: local_notebook.clone(),
        },
        ModelEvent::Snapshot(snapshot_1.clone()),
        ModelEvent::SaveBlock(completed_block_1.clone()),
        ModelEvent::Snapshot(snapshot_2.clone()),
        ModelEvent::SaveBlock(completed_block_2.clone()),
        ModelEvent::Snapshot(snapshot_3.clone()),
        ModelEvent::UpsertNotebook {
            notebook: local_notebook.clone(),
        },
    ];

    let filtered_events = deduplicate_events(original_events);
    assert_eq!(filtered_events.len(), 5);

    assert!(matches!(
        &filtered_events[0],
        &ModelEvent::UpsertNotebook { .. }
    ));
    // The first snapshot should have been filtered out.
    assert!(matches!(&filtered_events[1], &ModelEvent::SaveBlock(_)));
    // The second snapshot should have been filtered out.
    assert!(matches!(&filtered_events[2], &ModelEvent::SaveBlock(_)));
    // The third snapshot should be preserved.
    match &filtered_events[3] {
        ModelEvent::Snapshot(snapshot) => assert_eq!(snapshot, &snapshot_3),
        other => panic!("Expected ModelEvent::Snapshot, got {other:?}"),
    }
    assert!(matches!(
        &filtered_events[4],
        &ModelEvent::UpsertNotebook { .. }
    ));
}

#[test]
fn test_deduplicate_no_snapshots() {
    let original_events = vec![ModelEvent::SaveBlock(BlockCompleted {
        pane_id: vec![1, 2, 3],
        block: Default::default(),
        is_local: true,
    })];
    let filtered_events = deduplicate_events(original_events);
    assert_eq!(filtered_events.len(), 1);
    assert!(matches!(&filtered_events[0], &ModelEvent::SaveBlock(_)));
}

fn test_terminal_window_snapshot(vertical_tabs_panel_open: bool) -> WindowSnapshot {
    WindowSnapshot {
        tabs: vec![TabSnapshot {
            custom_title: None,
            root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                is_focused: true,
                custom_vertical_tabs_title: None,
                contents: LeafContents::Terminal(TerminalPaneSnapshot {
                    uuid: vec![u8::from(vertical_tabs_panel_open) + 1],
                    cwd: Some("/tmp".to_string()),
                    shell_launch_data: Some(ShellLaunchData::Executable {
                        executable_path: PathBuf::from("/bin/zsh"),
                        shell_type: crate::terminal::shell::ShellType::Zsh,
                    }),
                    is_active: true,
                    is_read_only: false,
                    input_config: None,
                    llm_model_override: None,
                    active_profile_id: None,
                    conversation_ids_to_restore: vec![],
                    active_conversation_id: None,
                    is_conversation_only: false,
                }),
            }),
            default_directory_color: None,
            selected_color: SelectedTabColor::default(),
            left_panel: None,
            right_panel: None,
            group_id: None,
            pinned: false,
        }],
        active_tab_index: 0,
        bounds: None,
        fullscreen_state: Default::default(),
        quake_mode: false,
        universal_search_width: None,
        warp_ai_width: None,
        voltron_width: None,
        warp_drive_index_width: None,
        left_panel_open: false,
        vertical_tabs_panel_open,
        left_panel_width: None,
        right_panel_width: None,
        cli_subagent_width: None,
        cli_subagent_height: None,
        agent_management_filters: None,
        theme_override: None,
        tab_groups: vec![],
    }
}

#[test]
fn test_sqlite_round_trips_vertical_tabs_panel_open() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let app_state = AppState {
        windows: vec![
            test_terminal_window_snapshot(false),
            test_terminal_window_snapshot(true),
        ],
        active_window_index: Some(1),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None)
        .expect("app state should load")
        .app_state;

    assert_eq!(restored.active_window_index, Some(1));
    assert_eq!(
        restored
            .windows
            .iter()
            .map(|window| window.vertical_tabs_panel_open)
            .collect::<Vec<_>>(),
        vec![false, true]
    );
}

fn test_workspace_metadata(path: &str) -> ai::workspace::WorkspaceMetadata {
    ai::workspace::WorkspaceMetadata {
        path: PathBuf::from(path),
        navigated_ts: None,
        modified_ts: None,
        queried_ts: None,
    }
}

/// Pin: `sqlite_read_restores_app_state_and_codebase_metadata`. The fork's
/// `read_sqlite_data` has no `PersistedDataScope` parameter (there is no
/// TUI/GUI database split -- see `PersistenceScope`/`PersistedDataScope` in
/// `DECLINED.md`'s "TUI/GUI shared app id" row) and always returns the full
/// data, so this drops that argument; everything else matches the pin.
#[test]
fn sqlite_read_restores_app_state_and_codebase_metadata() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let app_state = AppState {
        windows: vec![test_terminal_window_snapshot(false)],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };
    save_app_state(&mut conn, &app_state).expect("app state should save");

    let metadata = test_workspace_metadata("/tmp/remote-repo");
    save_codebase_index_metadata(&mut conn, metadata.clone())
        .expect("codebase index metadata should save");
    let restored = read_sqlite_data(&mut conn, None).expect("persisted data should load");
    assert_eq!(restored.app_state.windows.len(), 1);
    assert_eq!(restored.codebase_indices.len(), 1);
    assert_eq!(restored.codebase_indices[0].path, metadata.path);
}

/// Pin: `sqlite_writer_reuses_codebase_index_metadata_events`. Unlike
/// `tui_database_in_tui_subdirectory_round_trips_data` (not ported -- same
/// TUI/GUI database-split gap as above), this test never touches a `tui/`
/// subdirectory or a data scope, so it needs no adaptation beyond the fork's
/// plain `warp.sqlite` layout.
#[test]
fn sqlite_writer_reuses_codebase_index_metadata_events() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let conn = setup_database(&database_path).expect("database should initialize");

    let writer = start_writer(conn, database_path.clone()).expect("writer should start");
    let metadata = test_workspace_metadata("/tmp/writer-repo");
    writer
        .sender
        .send(ModelEvent::UpsertCodebaseIndexMetadata {
            index_metadata: Box::new(metadata.clone()),
        })
        .expect("upsert event should send");
    writer
        .sender
        .send(ModelEvent::Terminate)
        .expect("terminate event should send");
    writer.handle.join().expect("writer should terminate");

    let mut conn = setup_database(&database_path).expect("database should reopen");
    let restored = get_all_codebase_index_metadata(&mut conn).expect("metadata should load");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].path, metadata.path);

    let writer = start_writer(conn, database_path.clone()).expect("writer should restart");
    writer
        .sender
        .send(ModelEvent::DeleteCodebaseIndexMetadata {
            repo_path: metadata.path,
        })
        .expect("delete event should send");
    writer
        .sender
        .send(ModelEvent::Terminate)
        .expect("terminate event should send");
    writer.handle.join().expect("writer should terminate");

    let mut conn = setup_database(&database_path).expect("database should reopen");
    let restored = get_all_codebase_index_metadata(&mut conn).expect("metadata should load");
    assert!(restored.is_empty());
}

#[test]
fn test_sqlite_round_trips_custom_vertical_tabs_title() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![TabSnapshot {
                custom_title: None,
                root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: Some("Production API".to_string()),
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![42],
                        cwd: Some("/tmp".to_string()),
                        shell_launch_data: Some(ShellLaunchData::Executable {
                            executable_path: PathBuf::from("/bin/zsh"),
                            shell_type: crate::terminal::shell::ShellType::Zsh,
                        }),
                        is_active: true,
                        is_read_only: false,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        active_conversation_id: None,
                        is_conversation_only: false,
                    }),
                }),
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                left_panel: None,
                right_panel: None,
                group_id: None,
                pinned: false,
            }],
            active_tab_index: 0,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            left_panel_width: None,
            right_panel_width: None,
            cli_subagent_width: None,
            cli_subagent_height: None,
            agent_management_filters: None,
            theme_override: None,
            tab_groups: vec![],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None)
        .expect("app state should load")
        .app_state;

    let PaneNodeSnapshot::Leaf(LeafSnapshot {
        custom_vertical_tabs_title,
        ..
    }) = &restored.windows[0].tabs[0].root
    else {
        panic!("Expected terminal pane leaf");
    };
    assert_eq!(
        custom_vertical_tabs_title.as_deref(),
        Some("Production API")
    );
}

#[test]
fn test_sqlite_round_trips_code_pane_with_multiple_tabs() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![TabSnapshot {
                custom_title: None,
                root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Code(CodePaneSnapShot::Local {
                        tabs: vec![
                            CodePaneTabSnapshot {
                                path: Some(PathBuf::from("/tmp/main.rs")),
                            },
                            CodePaneTabSnapshot {
                                path: Some(PathBuf::from("/tmp/lib.rs")),
                            },
                            CodePaneTabSnapshot { path: None },
                        ],
                        active_tab_index: 1,
                        source: Some(CodeSource::FileTree {
                            path: PathBuf::from("/tmp/main.rs"),
                        }),
                    }),
                }),
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                left_panel: None,
                right_panel: None,
                group_id: None,
                pinned: false,
            }],
            active_tab_index: 0,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            left_panel_width: None,
            right_panel_width: None,
            cli_subagent_width: None,
            cli_subagent_height: None,
            agent_management_filters: None,
            theme_override: None,
            tab_groups: vec![],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None)
        .expect("app state should load")
        .app_state;

    assert_eq!(restored.windows.len(), 1);
    let restored_tab = &restored.windows[0].tabs[0];
    let PaneNodeSnapshot::Leaf(LeafSnapshot {
        contents:
            LeafContents::Code(CodePaneSnapShot::Local {
                tabs,
                active_tab_index,
                source,
            }),
        ..
    }) = &restored_tab.root
    else {
        panic!("Expected code pane leaf");
    };

    assert_eq!(tabs.len(), 3);
    assert_eq!(*active_tab_index, 1);
    assert_eq!(tabs[0].path, Some(PathBuf::from("/tmp/main.rs")));
    assert_eq!(tabs[1].path, Some(PathBuf::from("/tmp/lib.rs")));
    assert_eq!(tabs[2].path, None);
    assert!(matches!(source, Some(CodeSource::FileTree { .. })));
}

/// Verifies that a remote notebook pane (`NotebookPaneSnapshot::Remote`) round-trips through
/// save/restore with the same host and path it was opened with — the data restore reopens
/// against (`pane_group::restore_pane_leaf` calls `FilePane::new_remote(remote_path, ..)` with
/// exactly this value).
#[test]
fn test_sqlite_round_trips_remote_notebook_pane() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let remote_path = RemotePath::new(
        HostId::new("host-abc123".to_string()),
        StandardizedPath::try_new("/home/user/notes/readme.md").expect("valid standardized path"),
    );

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![TabSnapshot {
                custom_title: None,
                root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Notebook(NotebookPaneSnapshot::Remote {
                        remote_path: remote_path.clone(),
                    }),
                }),
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                left_panel: None,
                right_panel: None,
                group_id: None,
                pinned: false,
            }],
            active_tab_index: 0,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            left_panel_width: None,
            right_panel_width: None,
            cli_subagent_width: None,
            cli_subagent_height: None,
            agent_management_filters: None,
            theme_override: None,
            tab_groups: vec![],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None)
        .expect("app state should load")
        .app_state;

    assert_eq!(restored.windows.len(), 1);
    let restored_tab = &restored.windows[0].tabs[0];
    let PaneNodeSnapshot::Leaf(LeafSnapshot {
        contents:
            LeafContents::Notebook(NotebookPaneSnapshot::Remote {
                remote_path: restored_remote_path,
            }),
        ..
    }) = &restored_tab.root
    else {
        panic!("Expected remote notebook pane leaf");
    };

    assert_eq!(restored_remote_path, &remote_path);
    assert_eq!(
        restored_remote_path.host_id,
        HostId::new("host-abc123".to_string())
    );
    assert_eq!(
        restored_remote_path.path.as_str(),
        "/home/user/notes/readme.md"
    );
}

/// Verifies that a tab group and its membership round-trip through save/restore.
///
/// Ported from `warp/master`'s `test_sqlite_round_trips_tab_groups`, adapted to
/// the fork's `WindowSnapshot` (no `team_uid`; has `cli_subagent_*` /
/// `theme_override`) and its `read_sqlite_data(conn, user)` harness.
/// The most important test in `docs/design/moth-parliament.md` step 1's persistence work.
///
/// A conversation pane is deliberately never persisted (`LeafContents::is_persisted`'s
/// `LeafContents::Terminal(snapshot) => !snapshot.is_conversation_only` arm) -- there is no
/// `terminal_panes.kind` value for it and adding one needs a migration this branch does not
/// attempt. Before this branch, a conversation pane could only exist as a split alongside a
/// real terminal, so its surrounding tab always had at least one persisted leaf. This branch
/// makes a conversation pane a whole TAB by itself, which is new: a tab whose *entire* pane
/// tree is unpersisted.
///
/// `save_app_state`'s traversal (`persistence/sqlite.rs`) already `continue`s past a
/// non-persisted leaf before inserting its `pane_nodes` row, so a conversation-only tab gets
/// zero `pane_nodes` rows at all. On restore, `read_sqlite_data` builds each window's tabs
/// with `tabs_for_window.into_iter().filter_map(|tab| { let root =
/// read_root_node(conn, tab.id).ok()?; ... })` -- `read_root_node` fails with `NotFound`
/// (there is no row to find), `.ok()?` turns that into `None`, and `filter_map` drops just
/// that one tab, per tab, independently of its siblings. So a conversation-only tab quietly
/// not coming back is already safe with no extra guard: it disappears without taking any
/// other tab, or the window, down with it. This test proves that rather than asserting it,
/// per this branch's brief -- if the guard in `save_app_state` or the per-tab `filter_map` in
/// `read_sqlite_data` regressed (e.g. an unconditional `pane_nodes` insert, or hoisting the
/// `?` outside the closure so one bad tab fails the whole window), this would either grow
/// `windows[0].tabs` back to 2 or shrink it to 0.
#[test]
fn test_sqlite_conversation_only_tab_does_not_take_the_terminal_tab_down_with_it() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let terminal_tab = TabSnapshot {
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: true,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![7],
                cwd: Some("/tmp/survivor".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: true,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
                is_conversation_only: false,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: None,
        pinned: false,
    };
    let conversation_only_tab = TabSnapshot {
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: false,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![9],
                cwd: None,
                shell_launch_data: None,
                is_active: false,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
                is_conversation_only: true,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: None,
        pinned: false,
    };

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![terminal_tab, conversation_only_tab],
            active_tab_index: 0,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            left_panel_width: None,
            right_panel_width: None,
            cli_subagent_width: None,
            cli_subagent_height: None,
            agent_management_filters: None,
            theme_override: None,
            tab_groups: vec![],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None)
        .expect("app state should load")
        .app_state;

    assert_eq!(
        restored.windows.len(),
        1,
        "the window itself must survive intact"
    );
    let restored_window = &restored.windows[0];

    assert_eq!(
        restored_window.tabs.len(),
        1,
        "the conversation-only tab must not persist, and it must not take the terminal tab \
         down with it"
    );

    match &restored_window.tabs[0].root {
        PaneNodeSnapshot::Leaf(LeafSnapshot {
            contents: LeafContents::Terminal(snapshot),
            ..
        }) => {
            assert_eq!(
                snapshot.uuid,
                vec![7],
                "the surviving tab must be the terminal tab, not a corrupted/mixed-up row"
            );
            assert_eq!(snapshot.cwd.as_deref(), Some("/tmp/survivor"));
            assert!(!snapshot.is_conversation_only);
        }
        other => panic!("expected the surviving tab to be the terminal leaf, got {other:?}"),
    }
}

#[test]
fn test_sqlite_round_trips_tab_groups() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let group_id = TabGroupId::new();
    let tab_in_group = TabSnapshot {
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: true,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![1],
                cwd: Some("/tmp/grouped".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: true,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
                is_conversation_only: false,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: Some(group_id),
        pinned: false,
    };
    let tab_outside_group = TabSnapshot {
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: false,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![2],
                cwd: Some("/tmp/ungrouped".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: false,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
                is_conversation_only: false,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: None,
        pinned: false,
    };

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![tab_in_group, tab_outside_group],
            active_tab_index: 0,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            left_panel_width: None,
            right_panel_width: None,
            cli_subagent_width: None,
            cli_subagent_height: None,
            agent_management_filters: None,
            theme_override: None,
            tab_groups: vec![TabGroupSnapshot {
                id: group_id,
                name: Some("Backend".to_string()),
                color: SelectedTabColor::Color(AnsiColorIdentifier::Blue),
                collapsed: true,
                pinned: false,
            }],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None)
        .expect("app state should load")
        .app_state;

    assert_eq!(restored.windows.len(), 1);
    let restored_window = &restored.windows[0];
    assert_eq!(restored_window.tab_groups.len(), 1);
    let restored_group = &restored_window.tab_groups[0];
    assert_eq!(restored_group.name.as_deref(), Some("Backend"));
    assert_eq!(
        restored_group.color,
        SelectedTabColor::Color(AnsiColorIdentifier::Blue)
    );
    assert!(restored_group.collapsed);

    // The in-memory `TabGroupId` is minted fresh on restore, so we check that
    // the grouped tab points at the restored group, and the ungrouped tab
    // remains ungrouped.
    assert_eq!(restored_window.tabs.len(), 2);
    assert_eq!(restored_window.tabs[0].group_id, Some(restored_group.id));
    assert_eq!(restored_window.tabs[1].group_id, None);
}

/// Verifies that the `pinned` flag on tabs and tab groups round-trips through
/// save/restore so the user's pinned layout survives an app restart.
///
/// Ported from `warp/master`'s `test_sqlite_round_trips_pinned_state`, adapted
/// to the fork's `WindowSnapshot` and `read_sqlite_data` harness.
#[test]
fn test_sqlite_round_trips_pinned_state() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let pinned_group_id = TabGroupId::new();
    let unpinned_group_id = TabGroupId::new();

    let pinned_tab = TabSnapshot {
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: true,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![10],
                cwd: Some("/tmp/pinned".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: true,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
                is_conversation_only: false,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: None,
        pinned: true,
    };
    let unpinned_tab = TabSnapshot {
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: false,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![11],
                cwd: Some("/tmp/unpinned".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: false,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
                is_conversation_only: false,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: Some(unpinned_group_id),
        pinned: false,
    };
    let tab_in_pinned_group = TabSnapshot {
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: false,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![12],
                cwd: Some("/tmp/pinned-group".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: false,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
                is_conversation_only: false,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: Some(pinned_group_id),
        pinned: false,
    };

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![pinned_tab, tab_in_pinned_group, unpinned_tab],
            active_tab_index: 0,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            left_panel_width: None,
            right_panel_width: None,
            cli_subagent_width: None,
            cli_subagent_height: None,
            agent_management_filters: None,
            theme_override: None,
            tab_groups: vec![
                TabGroupSnapshot {
                    id: pinned_group_id,
                    name: Some("Pinned".to_string()),
                    color: SelectedTabColor::default(),
                    collapsed: false,
                    pinned: true,
                },
                TabGroupSnapshot {
                    id: unpinned_group_id,
                    name: Some("Loose".to_string()),
                    color: SelectedTabColor::default(),
                    collapsed: false,
                    pinned: false,
                },
            ],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None)
        .expect("app state should load")
        .app_state;

    assert_eq!(restored.windows.len(), 1);
    let restored_window = &restored.windows[0];

    // Tabs come back in insertion order; pinned flag should match what we saved.
    assert_eq!(restored_window.tabs.len(), 3);
    assert!(restored_window.tabs[0].pinned);
    assert!(!restored_window.tabs[1].pinned);
    assert!(!restored_window.tabs[2].pinned);

    // Both groups round-trip with their pinned state preserved. Group ids are
    // minted fresh on restore, so we look them up by name.
    assert_eq!(restored_window.tab_groups.len(), 2);
    let restored_pinned_group = restored_window
        .tab_groups
        .iter()
        .find(|group| group.name.as_deref() == Some("Pinned"))
        .expect("pinned group should restore");
    let restored_loose_group = restored_window
        .tab_groups
        .iter()
        .find(|group| group.name.as_deref() == Some("Loose"))
        .expect("unpinned group should restore");
    assert!(restored_pinned_group.pinned);
    assert!(!restored_loose_group.pinned);
}

// ── Settings pane persistence ───────────────────────────────────────────────
//
// Regression guard for issue #578. The settings pane used to persist
// `SettingsSection`'s `Display`, which is localized in this fork, and read it
// back with `FromStr`, which matched English literals. Any translated UI wrote
// a value that could not be parsed, so the user silently landed on the default
// section instead of the pane they left open. Persistence now round-trips the
// stable `persistence_key`, and still upgrades the legacy values.

fn settings_window_snapshot(current_page: SettingsSection) -> WindowSnapshot {
    WindowSnapshot {
        tabs: vec![TabSnapshot {
            custom_title: None,
            root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                is_focused: true,
                custom_vertical_tabs_title: None,
                contents: LeafContents::Settings(SettingsPaneSnapshot::Local {
                    current_page,
                    search_query: None,
                }),
            }),
            default_directory_color: None,
            selected_color: SelectedTabColor::default(),
            left_panel: None,
            right_panel: None,
            group_id: None,
            pinned: false,
        }],
        active_tab_index: 0,
        bounds: None,
        fullscreen_state: Default::default(),
        quake_mode: false,
        universal_search_width: None,
        warp_ai_width: None,
        voltron_width: None,
        warp_drive_index_width: None,
        left_panel_open: false,
        vertical_tabs_panel_open: false,
        left_panel_width: None,
        right_panel_width: None,
        cli_subagent_width: None,
        cli_subagent_height: None,
        agent_management_filters: None,
        theme_override: None,
        tab_groups: vec![],
    }
}

fn restored_settings_page(conn: &mut diesel::sqlite::SqliteConnection) -> SettingsSection {
    let restored = read_sqlite_data(conn, None)
        .expect("app state should load")
        .app_state;
    let PaneNodeSnapshot::Leaf(LeafSnapshot {
        contents: LeafContents::Settings(SettingsPaneSnapshot::Local { current_page, .. }),
        ..
    }) = &restored.windows[0].tabs[0].root
    else {
        panic!("Expected settings pane leaf");
    };
    *current_page
}

#[test]
fn test_sqlite_round_trips_every_settings_section() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    // Exhaustive on purpose: `SettingsSection::all()` is compile-time checked
    // in `settings_view/mod_tests.rs`, so a newly added section that forgets a
    // persistence key fails here instead of silently losing someone's pane.
    for section in SettingsSection::all() {
        let app_state = AppState {
            windows: vec![settings_window_snapshot(*section)],
            active_window_index: Some(0),
            block_lists: Default::default(),
            running_mcp_servers: Default::default(),
        };

        save_app_state(&mut conn, &app_state).expect("app state should save");

        assert_eq!(
            restored_settings_page(&mut conn),
            *section,
            "{section:?} did not survive persist -> read"
        );
    }
}

#[test]
fn test_sqlite_stores_the_stable_key_not_the_display_label() {
    use diesel::{QueryDsl, RunQueryDsl, SelectableHelper};

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    // Every section, so a variant whose key drifts back towards `Display` is
    // caught at the point it is written rather than only when it is read.
    for section in SettingsSection::all() {
        let app_state = AppState {
            windows: vec![settings_window_snapshot(*section)],
            active_window_index: Some(0),
            block_lists: Default::default(),
            running_mcp_servers: Default::default(),
        };
        save_app_state(&mut conn, &app_state).expect("app state should save");

        let stored: Vec<model::SettingsPane> = schema::settings_panes::dsl::settings_panes
            .select(model::SettingsPane::as_select())
            .load(&mut conn)
            .expect("settings panes should load");

        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored[0].current_page,
            section.persistence_key(),
            "{section:?} was stored as something other than its stable key; a \
             translated section name must never reach the database"
        );
    }
}

#[test]
fn test_sqlite_upgrades_legacy_settings_page_values() {
    use diesel::connection::SimpleConnection;

    // Rows written before stable keys existed hold whatever `Display`
    // produced: the English label on an English UI, and the translated label
    // otherwise. Both must still restore the right pane -- these users are the
    // reason the read path keeps a legacy fallback.
    let legacy_values = [
        // English labels, including one that `FromStr` never knew.
        ("Network", SettingsSection::Network),
        ("Keyboard shortcuts", SettingsSection::Keybindings),
        ("Phosphor Agent", SettingsSection::WarpAgent),
        (
            "Editor and Code Review",
            SettingsSection::EditorAndCodeReview,
        ),
        // Pre-rebrand names.
        ("Oz", SettingsSection::WarpAgent),
        ("Zap Drive", SettingsSection::ZapDrive),
        ("Phosphor Drive", SettingsSection::ZapDrive),
        // The zh-CN label for the Network page, which a real user already had
        // stored -- it is why `FromStr` grew a Chinese arm in the first place.
        ("网络", SettingsSection::Network),
    ];

    for (stored_value, expected) in legacy_values {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let database_path = tempdir.path().join("warp.sqlite");
        let mut conn = setup_database(&database_path).expect("database should initialize");

        // Write a normal snapshot, then rewrite the stored page as an older
        // build would have left it.
        let app_state = AppState {
            windows: vec![settings_window_snapshot(SettingsSection::Appearance)],
            active_window_index: Some(0),
            block_lists: Default::default(),
            running_mcp_servers: Default::default(),
        };
        save_app_state(&mut conn, &app_state).expect("app state should save");
        conn.batch_execute(&format!(
            "UPDATE settings_panes SET current_page = '{stored_value}';"
        ))
        .expect("legacy value should be written");

        assert_eq!(
            restored_settings_page(&mut conn),
            expected,
            "a settings pane stored as {stored_value:?} was lost"
        );
    }
}

fn assert_encode_then_decode_preserves_original_path(original_path: PathBuf) {
    let bytes = encode_path(original_path.clone());
    let decoded_path = decode_path(bytes);
    assert_eq!(original_path, decoded_path);
}

/// Test that a local path can be encoded and decoded. We use this when persisting a local
/// file path for notebooks in sqlite. We need this test because Windows `OsString`s are
/// often arbitrary sequences of 16-bit values, unlike Unix which uses sequences of 8-bit
/// values (bytes). Since `diesel::sql_types::Binary` deals with sequences of bytes (`u8`)
/// we need to perform special casting on `OsString`s on Windows.
#[test]
fn test_path_encode_decode() {
    // Empty path
    assert_encode_then_decode_preserves_original_path(PathBuf::new());

    // Windows-style paths
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"C:\windows\system32.dll"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("c:temp"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp\emoji\🙈.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp\ñoñàscii\temp.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp\hindi\हिन्दी"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp\cjk\狗没有耐心"));

    // Unix-style paths
    assert_encode_then_decode_preserves_original_path(PathBuf::from(
        "/home/persistence/example.sql",
    ));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("./database/log.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("/temp/emoji/🙈.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("/temp/ñoñàscii/temp.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("/temp/hindi/हिन्दी"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("/temp/cjk/狗没有耐心"));
}

#[cfg(target_os = "macos")]
const LEGACY_MIGRATION_MARKER_FILE: &str = ".zap-app-group-sqlite-migrated";

/// Reads the migration marker's payload, failing the test if it is absent.
#[cfg(target_os = "macos")]
fn read_migration_marker(state_dir: &std::path::Path) -> String {
    fs::read_to_string(state_dir.join(LEGACY_MIGRATION_MARKER_FILE))
        .expect("a migration marker should have been written")
}

#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_copies_newer_legacy_files() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let legacy_dir = tempdir.path().join("legacy");
    let state_dir = tempdir.path().join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&legacy_dir).expect("legacy dir should be created");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    fs::write(&target_db, "old-target").expect("target db should be written");
    std::thread::sleep(std::time::Duration::from_secs(1));

    let legacy_db = legacy_dir.join("warp.sqlite");
    fs::write(&legacy_db, "legacy-db").expect("legacy db should be written");
    fs::write(legacy_db.with_extension("sqlite-wal"), "legacy-wal")
        .expect("legacy wal should be written");
    fs::write(legacy_db.with_extension("sqlite-shm"), "legacy-shm")
        .expect("legacy shm should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &[legacy_dir])
        .expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "legacy-db");
    assert_eq!(
        fs::read_to_string(target_db.with_extension("sqlite-wal")).unwrap(),
        "legacy-wal"
    );
    assert_eq!(
        fs::read_to_string(target_db.with_extension("sqlite-shm")).unwrap(),
        "legacy-shm"
    );
    // The displaced database is moved aside, not destroyed.
    assert_eq!(
        fs::read_to_string(state_dir.join("warp.sqlite.replaced-by-app-group-migration")).unwrap(),
        "old-target"
    );
    assert_eq!(read_migration_marker(&state_dir), "v2 decided\n");
}

#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_copies_when_legacy_wal_is_newer() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let legacy_dir = tempdir.path().join("legacy");
    let state_dir = tempdir.path().join("state");
    let legacy_db = legacy_dir.join("warp.sqlite");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&legacy_dir).expect("legacy dir should be created");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    fs::write(&legacy_db, "legacy-db").expect("legacy db should be written");
    std::thread::sleep(std::time::Duration::from_secs(1));
    fs::write(&target_db, "target-db").expect("target db should be written");
    std::thread::sleep(std::time::Duration::from_secs(1));
    fs::write(legacy_db.with_extension("sqlite-wal"), "legacy-wal")
        .expect("legacy wal should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &[legacy_dir])
        .expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "legacy-db");
    assert_eq!(
        fs::read_to_string(target_db.with_extension("sqlite-wal")).unwrap(),
        "legacy-wal"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_stale_target_wal_does_not_survive_the_copy() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let legacy_dir = tempdir.path().join("legacy");
    let state_dir = tempdir.path().join("state");
    let legacy_db = legacy_dir.join("warp.sqlite");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&legacy_dir).expect("legacy dir should be created");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    // The live database has a WAL; the legacy one does not. Copying only the
    // main file would leave the *old* WAL beside the *new* database, which is
    // a corrupt pair rather than a migration.
    fs::write(&target_db, "target-db").expect("target db should be written");
    fs::write(target_db.with_extension("sqlite-wal"), "target-wal")
        .expect("target wal should be written");
    std::thread::sleep(std::time::Duration::from_secs(1));
    fs::write(&legacy_db, "legacy-db").expect("legacy db should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &[legacy_dir])
        .expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "legacy-db");
    assert!(
        !target_db.with_extension("sqlite-wal").exists(),
        "the replaced database's WAL must not be left beside the new database"
    );
    assert_eq!(
        fs::read_to_string(state_dir.join("warp.sqlite-wal.replaced-by-app-group-migration"))
            .unwrap(),
        "target-wal"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_marker_skips_copy() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let legacy_dir = tempdir.path().join("legacy");
    let state_dir = tempdir.path().join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&legacy_dir).expect("legacy dir should be created");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    // A legacy database that would otherwise win: newer than the live one, and
    // the live one is missing entirely. The settled marker still stops it.
    fs::write(legacy_dir.join("warp.sqlite"), "legacy-db").expect("legacy db should be written");
    fs::write(state_dir.join(LEGACY_MIGRATION_MARKER_FILE), "v2 decided\n")
        .expect("marker should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &[legacy_dir])
        .expect("migration should succeed");

    assert!(
        !target_db.exists(),
        "a settled marker must stop the search before the container is read"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_re_evaluates_a_marker_from_an_older_format() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let legacy_dir = tempdir.path().join("legacy");
    let state_dir = tempdir.path().join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&legacy_dir).expect("legacy dir should be created");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    fs::write(legacy_dir.join("warp.sqlite"), "legacy-db").expect("legacy db should be written");
    // The v1 payload, written by the build whose verdict this fix exists to
    // overturn. It carries no version and no verdict, so it must not be
    // honoured: a user already hit by the old behaviour gets one re-evaluation.
    fs::write(state_dir.join(LEGACY_MIGRATION_MARKER_FILE), "migrated\n")
        .expect("marker should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &[legacy_dir])
        .expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "legacy-db");
    assert_eq!(read_migration_marker(&state_dir), "v2 decided\n");
}

#[cfg(target_os = "macos")]
#[test]
fn test_legacy_migration_marker_round_trips_every_state() {
    use super::LegacyMigrationMarker;

    for state in [
        LegacyMigrationMarker::Decided,
        LegacyMigrationMarker::NoLegacyDb {
            attempts: 2,
            target_was_absent: true,
        },
        LegacyMigrationMarker::NoLegacyDb {
            attempts: 0,
            target_was_absent: false,
        },
    ] {
        assert_eq!(
            LegacyMigrationMarker::parse(&state.render()),
            Some(state),
            "marker payload `{}` should parse back to itself",
            state.render().trim_end()
        );
    }

    // Anything this build cannot read means "no record", which is what makes a
    // version bump re-open the migration instead of silently honouring a
    // verdict written by a build with a different idea of what the fields mean.
    for unreadable in ["", "migrated\n", "v1 decided\n", "v3 decided\n", "v2 wat\n"] {
        assert_eq!(LegacyMigrationMarker::parse(unreadable), None);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_legacy_app_group_sqlite_dirs_target_historical_app_ids() {
    use super::legacy_app_group_sqlite_dirs_for_home;

    let dirs = legacy_app_group_sqlite_dirs_for_home(std::path::Path::new("/Users/example"));
    let rendered = dirs
        .iter()
        .map(|dir| dir.display().to_string())
        .collect::<Vec<_>>();

    // The App Group container is keyed by the bundle id that wrote the data, so
    // the candidates must be the ids this fork shipped under before the current
    // one — newest first. `dev.warp.WarpOss` is the oldest of them and is *not*
    // upstream's: it is what `Channel::Oss` shipped under at the initial public
    // release (`0dbd3d567`), before `f41f4bfac` renamed it to OpenWarp.
    assert_eq!(
        rendered,
        vec![
            "/Users/example/Library/Group Containers/2BBY89MBSN.dev.warp/Library/Application Support/dev.zap.Zap".to_string(),
            "/Users/example/Library/Group Containers/2BBY89MBSN.dev.warp/Library/Application Support/dev.openwarp.OpenWarp".to_string(),
            "/Users/example/Library/Group Containers/2BBY89MBSN.dev.warp/Library/Application Support/dev.warp.WarpOss".to_string(),
        ]
    );

    // Regression guard for the defect this function replaces: the search used to
    // be built from `ChannelState::app_id()`, so it looked under the *current*
    // id, where no legacy data can ever exist.
    let current_app_id = warp_core::channel::ChannelState::app_id().to_string();
    assert!(
        !rendered.iter().any(|dir| dir.ends_with(&current_app_id)),
        "the live app id `{current_app_id}` must not be a legacy candidate: {rendered:?}"
    );

    // Upstream Warp's *commercial* channels belong to a different application.
    // The `dev.warp.` prefix alone is not the test — `dev.warp.WarpOss` above
    // carries it and is ours — so the exclusion is named channel by channel.
    for foreign in [
        "dev.warp.Warp",
        "dev.warp.WarpPreview",
        "dev.warp.WarpDev",
        "dev.warp.Warp-Stable",
    ] {
        assert!(
            !rendered.iter().any(|dir| dir.ends_with(foreign)),
            "Warp's own app data (`{foreign}`) must not be imported: {rendered:?}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_prefers_the_newest_app_id_directory() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let home = tempdir.path();
    let dirs = super::legacy_app_group_sqlite_dirs_for_home(home);
    let state_dir = home.join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    // Every historical container holds data; the newest app id must win.
    assert_eq!(dirs.len(), 3, "each historical app id needs a fixture");
    for (dir, contents) in dirs.iter().zip(["zap-db", "openwarp-db", "warposs-db"]) {
        fs::create_dir_all(dir).expect("legacy dir should be created");
        fs::write(dir.join("warp.sqlite"), contents).expect("legacy db should be written");
    }

    migrate_zap_app_group_sqlite_if_needed(&target_db, &dirs).expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "zap-db");
    assert_eq!(read_migration_marker(&state_dir), "v2 decided\n");
}

#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_falls_back_to_the_older_app_id_directory() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let home = tempdir.path();
    let dirs = super::legacy_app_group_sqlite_dirs_for_home(home);
    let state_dir = home.join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    // Only the oldest container has data. That is `dev.warp.WarpOss`, the id
    // the OSS channel shipped under before the OpenWarp rename — the case the
    // previous list could not reach at all.
    let oldest = dirs.last().expect("there should be a legacy candidate");
    assert!(oldest.ends_with("dev.warp.WarpOss"));
    fs::create_dir_all(oldest).expect("legacy dir should be created");
    fs::write(oldest.join("warp.sqlite"), "warposs-db").expect("legacy db should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &dirs).expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "warposs-db");
}

/// The defect this test exists for: launch 1 finds nothing, `init_db` then
/// creates the live database, and launch 2 finds the legacy database only
/// *after* the live one exists and is newer than it.
///
/// The version that wrote no marker on a miss fails here: with no record of
/// what launch 1 saw, launch 2 falls through to the timestamp comparison,
/// which the just-created live database always wins. The version before that
/// fails too, for the opposite reason — it marked the miss as settled and
/// never looked again.
///
/// Note the ordering: the legacy database is created *before* launch 1 and
/// only moved into place afterwards, so its mtime is genuinely older than the
/// live database's. A test that created it after launch 1 would leave mtime
/// pointing the right way by accident and pass against the broken code.
#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_rescues_a_source_that_arrives_after_the_first_launch() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let home = tempdir.path();
    let dirs = super::legacy_app_group_sqlite_dirs_for_home(home);
    let state_dir = home.join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    // The user's real history, sitting on the backup volume with an old mtime.
    let staged = home.join("staged-warp.sqlite");
    fs::write(&staged, "rescued-db").expect("staged db should be written");
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Launch 1: the restore has not delivered anything yet.
    migrate_zap_app_group_sqlite_if_needed(&target_db, &dirs).expect("migration should succeed");
    assert!(!target_db.exists());
    assert_eq!(
        read_migration_marker(&state_dir),
        "v2 no-legacy-db attempts=1 target-was-absent=true\n"
    );

    // ...and then `init_db` creates the live database, which is now newer than
    // anything the restore can deliver.
    fs::write(&target_db, "freshly-created-db").expect("target db should be written");

    // The restore completes. `rename` preserves the source's mtime.
    let legacy_dir = dirs.last().expect("there should be a legacy candidate");
    fs::create_dir_all(legacy_dir).expect("legacy dir should be created");
    fs::rename(&staged, legacy_dir.join("warp.sqlite")).expect("legacy db should be restored");

    // Launch 2: the rescue this whole migration exists for.
    migrate_zap_app_group_sqlite_if_needed(&target_db, &dirs).expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "rescued-db");
    assert_eq!(
        fs::read_to_string(state_dir.join("warp.sqlite.replaced-by-app-group-migration")).unwrap(),
        "freshly-created-db",
        "the database created between the two launches must be kept, not destroyed"
    );
    assert_eq!(read_migration_marker(&state_dir), "v2 decided\n");
}

/// The other half of the reconciliation: the search is not free, so it does not
/// run forever. Reading inside another app's App Group container is what macOS
/// prompts about, and almost no OSS user has legacy data at all.
#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_stops_searching_after_a_bounded_number_of_misses() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let home = tempdir.path();
    let dirs = super::legacy_app_group_sqlite_dirs_for_home(home);
    let state_dir = home.join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    for attempt in 1..=super::MAX_LEGACY_SCAN_ATTEMPTS {
        migrate_zap_app_group_sqlite_if_needed(&target_db, &dirs)
            .expect("migration should succeed");
        assert_eq!(
            read_migration_marker(&state_dir),
            format!("v2 no-legacy-db attempts={attempt} target-was-absent=true\n")
        );
        fs::write(&target_db, "freshly-created-db").expect("target db should be written");
    }

    // A source that turns up after the window has closed is not taken.
    let legacy_dir = dirs.last().expect("there should be a legacy candidate");
    fs::create_dir_all(legacy_dir).expect("legacy dir should be created");
    fs::write(legacy_dir.join("warp.sqlite"), "too-late-db").expect("legacy db should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &dirs).expect("migration should succeed");

    assert_eq!(
        fs::read_to_string(&target_db).unwrap(),
        "freshly-created-db"
    );
}

/// An install that already had a database the first time the search came up
/// empty can never be rescued into — a legacy database arriving later loses the
/// comparison by construction — so it settles immediately rather than paying
/// for a container read whose answer is already known.
#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_settles_at_once_when_the_live_db_predates_the_search() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let home = tempdir.path();
    let dirs = super::legacy_app_group_sqlite_dirs_for_home(home);
    let state_dir = home.join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&state_dir).expect("state dir should be created");
    fs::write(&target_db, "target-db").expect("target db should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &dirs).expect("migration should succeed");
    assert_eq!(
        read_migration_marker(&state_dir),
        "v2 no-legacy-db attempts=1 target-was-absent=false\n"
    );

    let legacy_dir = dirs.last().expect("there should be a legacy candidate");
    fs::create_dir_all(legacy_dir).expect("legacy dir should be created");
    fs::write(legacy_dir.join("warp.sqlite"), "legacy-db").expect("legacy db should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &dirs).expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "target-db");
}

#[cfg(target_os = "macos")]
#[test]
fn test_migrate_zap_app_group_sqlite_marks_a_rejected_legacy_db_as_settled() {
    use super::migrate_zap_app_group_sqlite_if_needed;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let legacy_dir = tempdir.path().join("legacy");
    let state_dir = tempdir.path().join("state");
    let target_db = state_dir.join("warp.sqlite");
    fs::create_dir_all(&legacy_dir).expect("legacy dir should be created");
    fs::create_dir_all(&state_dir).expect("state dir should be created");

    // The legacy database exists but the live one is newer and was never
    // created by this migration's own doing: a real decision was made, so it is
    // recorded and never revisited.
    fs::write(legacy_dir.join("warp.sqlite"), "legacy-db").expect("legacy db should be written");
    std::thread::sleep(std::time::Duration::from_secs(1));
    fs::write(&target_db, "target-db").expect("target db should be written");

    migrate_zap_app_group_sqlite_if_needed(&target_db, &[legacy_dir])
        .expect("migration should succeed");

    assert_eq!(fs::read_to_string(&target_db).unwrap(), "target-db");
    assert_eq!(read_migration_marker(&state_dir), "v2 decided\n");
}

#[test]
fn test_deserialize_corrupted_guests() {
    let _ = FeatureFlag::SharedWithMe.override_enabled(true);
    // Use a hardcoded timestamp to ensure this test works on systems with more-than-microsecond
    // precision.
    let permissions_ts_micros = 123456;
    let permissions_ts =
        ServerTimestamp::from_unix_timestamp_micros(permissions_ts_micros).unwrap();

    let db_permissions = ObjectPermissions {
        id: 42,
        object_metadata_id: 10,
        subject_type: "TEAM".to_string(),
        subject_id: Some("7".to_string()),
        subject_uid: "team_uid12345678912345".to_string(),
        permissions_last_updated_at: Some(permissions_ts_micros),
        // This is not a valid set of encoded object guests.
        object_guests: Some(vec![1, 2, 3]),
        anyone_with_link_access_level: None,
        anyone_with_link_source: None,
    };

    // The overall permissions should successfully convert, minus the object guests.
    let cloud_permissions = super::to_cloud_object_permissions(&db_permissions, None);
    assert_eq!(
        cloud_permissions,
        Some(StoredObjectPermissions {
            owner: Owner::Team {
                team_uid: crate::server::ids::ServerId::from_string_lossy("team_uid12345678912345"),
            },
            permissions_last_updated_ts: Some(permissions_ts),
            anyone_with_link: None,
            guests: vec![],
        })
    );
}

// Regression: GH#10083. The macOS green-tile button could leave a 1px-wide
// window bound in `AppContext::window_bounds`, which previously round-tripped
// through SQLite and restored as an unusable 1px sliver. Bounds below the
// platform minimum window size must be dropped on save.
#[test]
fn test_sqlite_drops_too_small_bounds_on_save() {
    use diesel::prelude::*;
    use pathfinder_geometry::rect::RectF;
    use pathfinder_geometry::vector::Vector2F;

    use crate::persistence::schema::windows;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let mut snapshot = test_terminal_window_snapshot(false);
    snapshot.bounds = Some(RectF::new(
        Vector2F::new(0.0, -1410.0),
        Vector2F::new(1.0, 1410.0),
    ));

    let app_state = AppState {
        windows: vec![snapshot],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    // Query the row directly so the assertion isolates the save guard and is
    // not masked by the read-side guard in `read_sqlite_data`.
    let row: (Option<f32>, Option<f32>, Option<f32>, Option<f32>) = windows::dsl::windows
        .select((
            windows::columns::window_width,
            windows::columns::window_height,
            windows::columns::origin_x,
            windows::columns::origin_y,
        ))
        .first(&mut conn)
        .expect("a windows row should have been inserted");

    assert_eq!(
        row,
        (None, None, None, None),
        "save-path guard must persist NULL bound columns for sub-minimum geometry"
    );
}

// Regression: GH#10083. Users whose warp.sqlite already contains a 1px row
// (because they hit the bug on an earlier build) must still recover to default
// geometry on next launch rather than restoring the sliver.
#[test]
fn test_sqlite_drops_too_small_bounds_on_read() {
    use diesel::connection::SimpleConnection;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    // Save with no bounds so a row exists, then corrupt it directly to bypass
    // the save-path guard and simulate a pre-existing bad row.
    let app_state = AppState {
        windows: vec![test_terminal_window_snapshot(false)],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };
    save_app_state(&mut conn, &app_state).expect("app state should save");

    conn.batch_execute(
        "UPDATE windows \
         SET window_width = 1.0, window_height = 1410.0, \
             origin_x = 0.0, origin_y = -1410.0",
    )
    .expect("corrupting update should succeed");

    let restored = read_sqlite_data(&mut conn, None)
        .expect("app state should load")
        .app_state;

    assert_eq!(restored.windows.len(), 1);
    assert!(
        restored.windows[0].bounds.is_none(),
        "tiny persisted bounds must be discarded on read so users recover from a corrupt DB"
    );
}
