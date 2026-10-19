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
    codebase_index_has_embeddings, decode_path, deduplicate_events, encode_path,
    get_all_codebase_index_metadata, read_sqlite_data, read_sqlite_data_counting, save_app_state,
    save_codebase_index_embeddings, save_codebase_index_metadata, setup_database,
    setup_database_or_quarantine, start_writer,
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

/// #911: tabs whose rows do not read back are counted, so the user is told
/// the restore was partial instead of getting fewer tabs in silence.
#[test]
fn damaged_tab_rows_are_counted_as_skipped() {
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

    let skipped = std::cell::Cell::new(0);
    read_sqlite_data_counting(&mut conn, None, &skipped).expect("loads");
    assert_eq!(skipped.get(), 0, "an intact database skips nothing");

    diesel::connection::SimpleConnection::batch_execute(
        &mut conn,
        "PRAGMA foreign_keys = OFF; delete from pane_nodes;",
    )
        .expect("rows deleted");
    let restored = read_sqlite_data_counting(&mut conn, None, &skipped).expect("loads");
    assert_eq!(skipped.get(), 1);
    assert!(restored.app_state.windows[0].tabs.is_empty());
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

fn wait_for_nonempty_wal(wal: &std::path::Path) {
    for _ in 0..100 {
        if std::fs::metadata(wal).is_ok_and(|m| m.len() > 0) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// #920: after logout (pause and remove) and login (reconstruct), nothing may
/// unlink the new database's `-wal`.
#[test]
fn reconstruct_after_pause_keeps_the_new_databases_wal() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let conn = setup_database(&database_path).expect("database should initialize");
    let writer = start_writer(conn, database_path.clone()).expect("writer should start");

    let metadata = test_workspace_metadata("/tmp/after-logout");
    for event in [
        ModelEvent::PauseAndRemoveDatabase,
        ModelEvent::ReconstructAndResume,
        ModelEvent::UpsertCodebaseIndexMetadata {
            index_metadata: Box::new(metadata.clone()),
        },
    ] {
        writer.sender.send(event).expect("event should send");
    }
    // A Terminate would checkpoint and remove the WAL we want to see.
    let wal = tempdir.path().join("warp.sqlite-wal");
    wait_for_nonempty_wal(&wal);
    assert!(wal.exists(), "the new database's WAL must still exist");

    let mut reader = setup_database(&database_path).expect("a fresh connection should open");
    let mut restored = Vec::new();
    for _ in 0..100 {
        restored =
            get_all_codebase_index_metadata(&mut reader).expect("row should be readable");
        if !restored.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].path, metadata.path);
    assert!(wal.exists(), "the WAL must still exist after the read");

    writer.sender.send(ModelEvent::Terminate).expect("terminate");
    writer.handle.join().expect("writer should terminate");
}

/// #920: pausing closes the connection and resuming opens one, so repeated
/// logout/login cycles do not accumulate file descriptors.
#[cfg(target_os = "linux")]
#[test]
fn pause_and_reconstruct_cycles_do_not_leak_file_descriptors() {
    fn open_fds() -> usize {
        std::fs::read_dir("/proc/self/fd").map_or(0, |d| d.count())
    }
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let conn = setup_database(&database_path).expect("database should initialize");
    let writer = start_writer(conn, database_path.clone()).expect("writer should start");
    let cycle = |writer: &super::WriterHandles| {
        writer.sender.send(ModelEvent::PauseAndRemoveDatabase).unwrap();
        writer.sender.send(ModelEvent::ReconstructAndResume).unwrap();
        // The writer is idle again once it answers a no-op-ish event; a write
        // forces it through the preceding events.
        writer
            .sender
            .send(ModelEvent::DeleteCodebaseIndexMetadata {
                repo_path: PathBuf::from("/tmp/none"),
            })
            .unwrap();
    };
    for _ in 0..3 {
        cycle(&writer);
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let baseline = open_fds();
    for _ in 0..20 {
        cycle(&writer);
    }
    std::thread::sleep(std::time::Duration::from_millis(3000));
    let after = open_fds();
    writer.sender.send(ModelEvent::Terminate).unwrap();
    writer.handle.join().unwrap();
    assert!(
        after <= baseline + 2,
        "fds grew from {baseline} to {after} across 20 cycles"
    );
}

/// #911: a connection whose file is gone is closed (nothing to protect, no
/// leak); one whose path now holds something else is left open on purpose.
#[test]
fn abandoned_connection_is_closed_when_the_path_is_empty_and_leaked_when_occupied() {
    use super::retire_abandoned_connection;
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");

    let conn = setup_database(&database_path).expect("database should initialize");
    for suffix in ["", "-wal", "-shm"] {
        let mut name = database_path.as_os_str().to_owned();
        name.push(suffix);
        let _ = std::fs::remove_file(name);
    }
    assert!(!retire_abandoned_connection(conn, &database_path), "closed");

    let conn = setup_database(&database_path).expect("database should initialize");
    std::fs::remove_file(&database_path).unwrap();
    std::fs::write(&database_path, b"someone else's file").unwrap();
    assert!(retire_abandoned_connection(conn, &database_path), "leaked");
    assert_eq!(
        std::fs::read(&database_path).unwrap(),
        b"someone else's file",
        "the file at the path is untouched"
    );
}

/// Backs `EmbeddingEndpoints::preferred_model_favoring_existing`
/// (`ai/agent_providers/embeddings.rs`): whether a model has ever been
/// embedded against is answered per `embedding_space`
/// (`EmbeddingConfig::storage_key()`), not globally, and only by rows in
/// `codebase_index_embeddings` -- a `codebase_index_nodes` row alone (recorded
/// for an unembedded merkle subtree) must not count as "has an index".
#[test]
fn codebase_index_has_embeddings_is_scoped_to_its_own_space() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    assert!(
        !codebase_index_has_embeddings(&mut conn, "voyage:voyage-4:512")
            .expect("query should succeed"),
        "an empty database must report no existing index for any space"
    );

    save_codebase_index_embeddings(
        &mut conn,
        "voyage:voyage-4:512".to_string(),
        vec![("content-hash-1".to_string(), 512, vec![0u8; 4])],
    )
    .expect("saving an embedding should succeed");

    assert!(
        codebase_index_has_embeddings(&mut conn, "voyage:voyage-4:512")
            .expect("query should succeed"),
        "a space with a saved embedding must report an existing index"
    );
    assert!(
        !codebase_index_has_embeddings(&mut conn, "openai:text-embedding-3-small:256")
            .expect("query should succeed"),
        "a different space must not see another space's rows"
    );
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

/// #860: random bytes in `warp.sqlite` must not disable persistence forever.
/// The file is moved aside (not deleted) and a working database is created.
#[test]
fn corrupt_database_is_moved_aside_and_recreated() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let garbage = b"this is definitely not a sqlite database, just text".repeat(200);
    std::fs::write(&database_path, &garbage).expect("garbage written");

    let mut conn =
        setup_database_or_quarantine(&database_path).expect("a fresh database should be created");
    read_sqlite_data(&mut conn, None).expect("the fresh database should be readable");

    let moved: Vec<_> = std::fs::read_dir(tempdir.path())
        .expect("read_dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("warp.sqlite.corrupt-"))
        })
        .collect();
    assert_eq!(moved.len(), 1, "the unusable file should be kept aside");
    assert_eq!(std::fs::read(&moved[0]).expect("read moved"), garbage);
}

/// #860: a directory where the database should be is also moved aside.
#[test]
fn directory_in_place_of_database_is_moved_aside() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    std::fs::create_dir(&database_path).expect("directory created");

    setup_database_or_quarantine(&database_path).expect("a fresh database should be created");
    assert!(database_path.is_file());
}

/// A healthy database is left alone.
#[test]
fn healthy_database_is_not_quarantined() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    drop(setup_database(&database_path).expect("database should initialize"));

    setup_database_or_quarantine(&database_path).expect("reopen");
    let entries = std::fs::read_dir(tempdir.path())
        .expect("read_dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
        .count();
    assert_eq!(entries, 0);
}

#[test]
fn only_content_errors_mean_an_unusable_database() {
    assert!(super::error_means_unusable_database(
        "database disk image is malformed"
    ));
    assert!(super::error_means_unusable_database(
        "file is not a database"
    ));
    // A newer build's schema, or a migration bug, must never trigger a move.
    assert!(!super::error_means_unusable_database(
        "no such table: blocks"
    ));
    assert!(!super::error_means_unusable_database("no such column: foo"));
    // Transient or environmental conditions must never trigger a move.
    assert!(!super::error_means_unusable_database("database is locked"));
    assert!(!super::error_means_unusable_database(
        "database or disk is full"
    ));
    assert!(!super::error_means_unusable_database("disk i/o error"));
}

fn corrupt_copies(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .expect("read_dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.to_string_lossy().contains(".corrupt-"))
        .collect();
    found.sort();
    found
}

/// #860: a database migrated by a newer build (a migration this build does not
/// know) is healthy and must never be quarantined, even if a read fails with
/// "no such column".
#[test]
fn database_with_unknown_future_migration_is_not_quarantined() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    use diesel::connection::SimpleConnection;
    let mut conn = setup_database(&database_path).expect("database should initialize");
    conn.batch_execute(
        "INSERT INTO __diesel_schema_migrations (version) VALUES ('99991231235959')",
    )
    .expect("future migration recorded");
    drop(conn);

    assert!(super::database_has_unknown_migrations(&database_path));
    assert!(super::quarantine_unusable_database(&database_path).is_err());
    assert!(database_path.is_file());
    assert!(corrupt_copies(tempdir.path()).is_empty());
    assert!(!super::error_means_unusable_database(
        "no such column: future_column"
    ));
}

/// A database created in this process is never quarantined.
#[test]
fn freshly_created_database_is_not_quarantined() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    drop(setup_database_or_quarantine(&database_path).expect("created"));
    assert!(super::quarantine_unusable_database(&database_path).is_err());
    assert!(database_path.is_file());
    assert!(corrupt_copies(tempdir.path()).is_empty());
}

/// Two quarantines in the same second must keep both files.
#[test]
fn quarantines_in_the_same_second_do_not_overwrite_each_other() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    std::fs::write(&database_path, b"first").expect("write");
    std::fs::write(tempdir.path().join("warp.sqlite-wal"), b"first-wal").expect("write");
    let first = super::quarantine_database_files(&database_path, 100).expect("first");
    std::fs::write(&database_path, b"second").expect("write");
    let second = super::quarantine_database_files(&database_path, 100).expect("second");

    assert_ne!(first, second);
    assert_eq!(std::fs::read(&first).expect("read"), b"first");
    assert_eq!(std::fs::read(&second).expect("read"), b"second");
    let wal = tempdir.path().join("warp.sqlite.corrupt-100-wal");
    assert_eq!(std::fs::read(wal).expect("wal moved"), b"first-wal");
    assert!(!database_path.exists());
}

/// A failed sidecar rename aborts the quarantine and leaves the original.
#[test]
fn failed_sidecar_rename_leaves_the_original_in_place() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    std::fs::write(&database_path, b"main").expect("write");
    std::fs::write(tempdir.path().join("warp.sqlite-shm"), b"shm").expect("write");
    std::fs::write(tempdir.path().join("warp.sqlite-wal"), b"wal").expect("write");

    let result = super::quarantine_database_files_with(&database_path, 7, &|from, to| {
        if to.to_string_lossy().ends_with("-shm") {
            Err(std::io::Error::other("injected failure"))
        } else {
            std::fs::rename(from, to)
        }
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&database_path).expect("main kept"), b"main");
    assert_eq!(
        std::fs::read(tempdir.path().join("warp.sqlite-wal")).expect("wal restored after rollback"),
        b"wal"
    );
    assert_eq!(
        std::fs::read(tempdir.path().join("warp.sqlite-shm")).expect("shm kept"),
        b"shm"
    );
    assert!(corrupt_copies(tempdir.path()).is_empty());
}

/// A transient-looking error on a database with a valid header and a passing
/// quick_check is not damage.
#[test]
fn transient_error_on_valid_database_is_not_damage() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    drop(setup_database(&database_path).expect("database should initialize"));
    assert_eq!(super::has_valid_sqlite_header(&database_path), Some(true));
    assert!(super::error_may_be_transient_io("disk i/o error"));
    assert!(!super::transient_error_confirms_damage(&database_path));

    let garbage = tempdir.path().join("garbage.sqlite");
    std::fs::write(&garbage, b"definitely not sqlite, long enough header").expect("write");
    assert!(super::transient_error_confirms_damage(&garbage));
}

/// Bytes of every regular file under `dir`, to prove nothing was moved,
/// rewritten or deleted.
fn snapshot_files(dir: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(dir)
        .expect("read_dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).expect("read"),
            )
        })
        .collect()
}

/// #860: a read-only database file is healthy. Whatever opening it does, it
/// must never be quarantined or modified.
#[cfg(unix)]
#[test]
fn read_only_database_file_is_never_quarantined() {
    use std::os::unix::fs::PermissionsExt;
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    drop(setup_database(&database_path).expect("database should initialize"));
    std::fs::set_permissions(&database_path, std::fs::Permissions::from_mode(0o444))
        .expect("chmod");
    let before = snapshot_files(tempdir.path());

    let result = setup_database_or_quarantine(&database_path);
    if let Err(err) = &result {
        // The failure is reported to the user as a read-only problem.
        let message = super::underlying_error_message(err);
        let running_as_root = std::fs::OpenOptions::new()
            .write(true)
            .open(&database_path)
            .is_ok();
        if !running_as_root {
            assert_eq!(
                super::super::database_notice::classify_database_problem(&message, &database_path),
                super::super::database_notice::DatabaseProblem::ReadOnly,
                "read-only database reported as: {message}"
            );
        }
    }
    drop(result);

    assert!(corrupt_copies(tempdir.path()).is_empty());
    assert!(database_path.is_file());
    let after = snapshot_files(tempdir.path());
    assert_eq!(
        before.get("warp.sqlite"),
        after.get("warp.sqlite"),
        "the database file must be byte-for-byte unchanged"
    );
}

/// #860: a healthy database in a read-only folder (SQLite cannot create its
/// -wal/-shm files) is also never quarantined.
#[cfg(unix)]
#[test]
fn database_in_read_only_folder_is_never_quarantined() {
    use std::os::unix::fs::PermissionsExt;
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let folder = tempdir.path().join("state");
    std::fs::create_dir(&folder).expect("mkdir");
    let database_path = folder.join("warp.sqlite");
    drop(setup_database(&database_path).expect("database should initialize"));
    let before = snapshot_files(&folder);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).expect("chmod");

    let result = setup_database_or_quarantine(&database_path);
    drop(result);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    assert!(corrupt_copies(&folder).is_empty());
    assert_eq!(
        before.get("warp.sqlite"),
        snapshot_files(&folder).get("warp.sqlite")
    );
}

/// #860: full-disk and read-only errors never mean the file is damaged.
#[test]
fn full_disk_and_read_only_errors_are_not_damage() {
    for message in [
        "database or disk is full",
        "attempt to write a readonly database",
        "unable to open database file",
        "no space left on device",
    ] {
        assert!(!super::error_means_unusable_database(message), "{message}");
    }
}

/// #860: a database cut off half way through is moved aside and replaced, or
/// (if SQLite can still open it) fails with an error that the startup read
/// treats as damage -- never a silent, permanent loss of persistence.
#[test]
fn half_truncated_database_is_recovered() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    drop(setup_database(&database_path).expect("database should initialize"));
    let length = std::fs::metadata(&database_path).expect("metadata").len();
    assert!(length > 8192, "the migrated schema spans many pages");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&database_path)
        .expect("open")
        .set_len(length / 2)
        .expect("truncate");

    let mut conn = setup_database_or_quarantine(&database_path)
        .expect("a truncated database must not disable persistence");
    match read_sqlite_data(&mut conn, None) {
        Ok(_) => {}
        Err(err) => assert!(
            super::error_means_unusable_database(&super::diesel_database_error_message(&err)),
            "a truncated database's read error must lead to quarantine, got: {err}"
        ),
    }
}

/// #902: the prewarm thread and a second opener migrating one empty database
/// at the same time must all succeed, with the schema applied exactly once and
/// nothing quarantined.
#[test]
fn concurrent_setup_of_a_fresh_database_all_succeed() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let path = database_path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                setup_database_or_quarantine(&path)
                    .map(drop)
                    .map_err(|e| format!("{e:#}"))
            })
        })
        .collect();
    for handle in handles {
        handle
            .join()
            .expect("thread should not panic")
            .expect("a concurrent migrator must not fail");
    }
    assert_database_fully_migrated_and_unquarantined(tempdir.path(), &database_path);
}

fn assert_database_fully_migrated_and_unquarantined(dir: &std::path::Path, db: &std::path::Path) {
    use diesel::{Connection, SqliteConnection};
    use diesel_migrations::MigrationHarness;
    let mut conn = SqliteConnection::establish(db.to_str().unwrap()).expect("open");
    assert!(
        !conn
            .has_pending_migration(::persistence::MIGRATIONS)
            .expect("check pending"),
        "every migration should be applied"
    );
    let quarantined = std::fs::read_dir(dir)
        .expect("read_dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
        .count();
    assert_eq!(quarantined, 0, "a migration race must never quarantine");
}

/// Child half of [`concurrent_processes_setup_of_a_fresh_database_all_succeed`]:
/// does nothing unless the parent set the environment variable.
#[test]
fn migration_race_child_process() {
    let Some(path) = std::env::var_os("PHOSPHOR_TEST_MIGRATION_RACE_DB") else {
        return;
    };
    let path = PathBuf::from(path);
    let go = path.with_extension("go");
    while !go.exists() {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    setup_database_or_quarantine(&path).expect("a concurrent migrator must not fail");
}

/// #902 across processes: several processes migrating one empty database at
/// once must all succeed.
#[test]
// Test-only: re-executes this test binary, which `crates/command` cannot do.
#[allow(clippy::disallowed_types)]
fn concurrent_processes_setup_of_a_fresh_database_all_succeed() {
    if std::env::var_os("PHOSPHOR_TEST_MIGRATION_RACE_DB").is_some() {
        return;
    }
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let exe = std::env::current_exe().expect("current exe");
    let children: Vec<_> = (0..4)
        .map(|_| {
            std::process::Command::new(&exe)
                .args([
                    "--exact",
                    "persistence::sqlite::tests::migration_race_child_process",
                ])
                .arg("--test-threads=1")
                .env("PHOSPHOR_TEST_MIGRATION_RACE_DB", &database_path)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("spawn child")
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(500));
    std::fs::write(database_path.with_extension("go"), b"").expect("go file");
    for child in children {
        let output = child.wait_with_output().expect("child output");
        assert!(
            output.status.success(),
            "child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_database_fully_migrated_and_unquarantined(tempdir.path(), &database_path);
}

/// #902: a lost migration race is recognised, and is not "unusable".
#[test]
fn migration_race_error_is_recognised_and_not_unusable() {
    let message = "unique constraint failed: __diesel_schema_migrations.version";
    assert!(super::error_is_migration_race(message));
    assert!(!super::error_means_unusable_database(message));
    assert!(!super::error_may_be_transient_io(message));
    assert!(!super::error_is_migration_race("malformed database schema"));
}

/// #902: setup runs on the main thread at startup, so a lock held by someone
/// else (a second instance, a user's `sqlite3` with `BEGIN EXCLUSIVE`) must
/// fail it within the shared budget instead of freezing the UI -- and must not
/// make the database look unusable.
#[test]
// Test-only: measures wall-clock time against the setup lock budget.
#[allow(clippy::disallowed_types)]
fn setup_gives_up_within_the_budget_when_another_connection_holds_the_lock() {
    use diesel::{connection::SimpleConnection, Connection, SqliteConnection};
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut holder = SqliteConnection::establish(database_path.to_str().unwrap()).expect("open");
    holder
        .batch_execute("PRAGMA journal_mode=DELETE; CREATE TABLE held (x); BEGIN EXCLUSIVE;")
        .expect("take the exclusive lock");

    let started = std::time::Instant::now();
    let result = setup_database_or_quarantine(&database_path);
    let elapsed = started.elapsed();

    assert!(result.is_err(), "setup cannot succeed while the lock is held");
    assert!(
        elapsed < std::time::Duration::from_millis(3500),
        "setup must give up within the budget, took {elapsed:?}"
    );
    let quarantined = std::fs::read_dir(tempdir.path())
        .expect("read_dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
        .count();
    assert_eq!(quarantined, 0, "a held lock must never quarantine the database");

    holder.batch_execute("ROLLBACK;").expect("release");
    drop(holder);
    setup_database_or_quarantine(&database_path).expect("setup works once the lock is gone");
}

// ---------------------------------------------------------------------------
// #936: the commands-table trim must hold the cap even when the table is already over it.
// Nothing references `commands.id` (no foreign key, no other table stores a command id), so
// the trim has no referenced rows to protect.
// ---------------------------------------------------------------------------

fn commands_test_connection(rows: i64) -> diesel::sqlite::SqliteConnection {
    use diesel::{Connection, SqliteConnection, connection::SimpleConnection};
    use diesel_migrations::MigrationHarness as _;
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory sqlite");
    conn.run_pending_migrations(::persistence::MIGRATIONS)
        .expect("migrations");
    conn.batch_execute(&format!(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {rows})
         INSERT INTO commands (id, command) SELECT i, 'cmd' || i FROM n;"
    ))
    .expect("seed commands");
    conn
}

fn started_command(command: &str) -> crate::persistence::StartedCommandMetadata {
    crate::persistence::StartedCommandMetadata {
        command: command.to_string(),
        start_ts: None,
        pwd: None,
        shell: None,
        username: None,
        hostname: None,
        session_id: None,
        git_branch: None,
        cloud_workflow_id: None,
        workflow_command: None,
        is_agent_executed: false,
    }
}

fn command_stats(conn: &mut diesel::sqlite::SqliteConnection) -> (i64, i64, i64) {
    use diesel::{
        QueryDsl, RunQueryDsl,
        dsl::{count_star, max, min},
    };
    use schema::commands::dsl::*;
    let n: i64 = commands.select(count_star()).first(conn).unwrap();
    let lo: Option<i32> = commands.select(min(id)).first(conn).unwrap();
    let hi: Option<i32> = commands.select(max(id)).first(conn).unwrap();
    (n, lo.unwrap() as i64, hi.unwrap() as i64)
}

#[test]
fn insert_command_trims_an_oversized_table_to_the_cap_keeping_the_newest() {
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    let mut conn = commands_test_connection(50_000);
    super::insert_command(&mut conn, started_command("new")).expect("insert");

    let (n, lo, hi) = command_stats(&mut conn);
    assert_eq!(n, super::COMMANDS_COUNT_LIMIT);
    assert_eq!(hi, 50_001, "the new row is kept");
    assert_eq!(
        lo,
        50_001 - super::COMMANDS_COUNT_LIMIT + 1,
        "the newest rows are kept"
    );
    let newest: String = schema::commands::dsl::commands
        .select(schema::commands::dsl::command)
        .filter(schema::commands::dsl::id.eq(50_001))
        .first(&mut conn)
        .unwrap();
    assert_eq!(newest, "new");
}

#[test]
fn insert_command_catches_up_a_huge_table_in_bounded_passes() {
    let mut conn = commands_test_connection(120_000);
    super::insert_command(&mut conn, started_command("a")).expect("insert");
    let (n, _, _) = command_stats(&mut conn);
    assert_eq!(
        n,
        120_001 - super::COMMANDS_TRIM_BATCH_LIMIT,
        "one pass is bounded"
    );
    for _ in 0..3 {
        super::insert_command(&mut conn, started_command("b")).expect("insert");
    }
    let (n, _, _) = command_stats(&mut conn);
    assert_eq!(n, super::COMMANDS_COUNT_LIMIT);
}

#[test]
fn insert_command_at_the_cap_evicts_exactly_the_oldest() {
    let mut conn = commands_test_connection(super::COMMANDS_COUNT_LIMIT);
    super::insert_command(&mut conn, started_command("new")).expect("insert");
    let (n, lo, hi) = command_stats(&mut conn);
    assert_eq!(
        (n, lo, hi),
        (
            super::COMMANDS_COUNT_LIMIT,
            2,
            super::COMMANDS_COUNT_LIMIT + 1
        )
    );
}

#[test]
fn insert_command_under_the_cap_does_not_trim() {
    let mut conn = commands_test_connection(super::COMMANDS_COUNT_LIMIT - 1);
    super::insert_command(&mut conn, started_command("new")).expect("insert");
    let (n, lo, _) = command_stats(&mut conn);
    assert_eq!((n, lo), (super::COMMANDS_COUNT_LIMIT, 1));
    let mut small = commands_test_connection(5);
    super::insert_command(&mut small, started_command("new")).expect("insert");
    assert_eq!(command_stats(&mut small), (6, 1, 6));
}
