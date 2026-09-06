use std::path::PathBuf;

use crate::{
    app_state::{
        AppState, BranchSnapshot, LeafContents, LeafSnapshot, NotebookPaneSnapshot, PaneFlex,
        PaneNodeSnapshot, SplitDirection, TabSnapshot, TerminalPaneSnapshot, WindowSnapshot,
    },
    drive::ZapDriveObjectSettings,
    tab::SelectedTabColor,
};

use super::{CommandTemplate, LaunchConfig, PaneMode, PaneTemplateType};

fn single_tab_snapshot(root: PaneNodeSnapshot) -> AppState {
    AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                group_id: None,
                pinned: false,
                root,
                left_panel: None,
                right_panel: None,
            }],
            active_tab_index: 0,
            bounds: None,
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            fullscreen_state: Default::default(),
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
    }
}

fn multi_tab_snapshot(active_tab_index: usize, tabs: Vec<TabSnapshot>) -> AppState {
    AppState {
        windows: vec![WindowSnapshot {
            tabs,
            active_tab_index,
            bounds: None,
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            fullscreen_state: Default::default(),
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
    }
}

#[test]
fn test_config_from_snapshot_flattens_single_pane() {
    // If only one pane of the branch can be saved into a launch configuration, it should
    // be flattened to a single leaf.

    let state = single_tab_snapshot(PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Vertical,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Notebook(NotebookPaneSnapshot::NotebookObject {
                        notebook_id: None,
                        settings: ZapDriveObjectSettings::default(),
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![],
                        cwd: Some("/some/dir".into()),
                        is_active: true,
                        is_read_only: false,
                        shell_launch_data: None,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        active_conversation_id: None,
                        is_conversation_only: false,
                    }),
                }),
            ),
        ],
    }));

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(
        template.windows[0].tabs[0].layout,
        PaneTemplateType::PaneTemplate {
            is_focused: Some(true),
            cwd: PathBuf::from("/some/dir"),
            commands: vec![],
            pane_mode: PaneMode::Terminal,
            shell: None,
        },
    )
}

#[test]
fn test_config_from_snapshot_filters_panes() {
    let state = single_tab_snapshot(PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Vertical,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![],
                        cwd: Some("/path/to/dir".into()),
                        is_active: true,
                        is_read_only: false,
                        shell_launch_data: None,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        active_conversation_id: None,
                        is_conversation_only: false,
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Notebook(NotebookPaneSnapshot::NotebookObject {
                        notebook_id: None,
                        settings: ZapDriveObjectSettings::default(),
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![],
                        cwd: Some("/some/dir".into()),
                        is_active: true,
                        is_read_only: false,
                        shell_launch_data: None,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        active_conversation_id: None,
                        is_conversation_only: false,
                    }),
                }),
            ),
        ],
    }));

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(
        template.windows[0].tabs[0].layout,
        PaneTemplateType::PaneBranchTemplate {
            split_direction: SplitDirection::Vertical.into(),
            panes: vec![
                PaneTemplateType::PaneTemplate {
                    is_focused: Some(true),
                    cwd: PathBuf::from("/path/to/dir"),
                    commands: vec![],
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
                PaneTemplateType::PaneTemplate {
                    is_focused: Some(false),
                    cwd: PathBuf::from("/some/dir"),
                    commands: vec![],
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
            ]
        }
    )
}

#[test]
fn test_config_from_snapshot_filters_tabs() {
    // If no panes of a tab are valid, it's filtered out entirely.

    let state = single_tab_snapshot(PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Vertical,
        children: vec![(
            PaneFlex(1.),
            PaneNodeSnapshot::Leaf(LeafSnapshot {
                is_focused: true,
                custom_vertical_tabs_title: None,
                contents: LeafContents::Notebook(NotebookPaneSnapshot::NotebookObject {
                    notebook_id: None,
                    settings: ZapDriveObjectSettings::default(),
                }),
            }),
        )],
    }));

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert!(template.windows[0].tabs.is_empty())
}

fn conversation_only_leaf(is_focused: bool) -> PaneNodeSnapshot {
    PaneNodeSnapshot::Leaf(LeafSnapshot {
        is_focused,
        custom_vertical_tabs_title: None,
        contents: LeafContents::Terminal(TerminalPaneSnapshot {
            uuid: vec![],
            cwd: Some("/some/dir".into()),
            is_active: true,
            is_read_only: false,
            shell_launch_data: None,
            input_config: None,
            llm_model_override: None,
            active_profile_id: None,
            conversation_ids_to_restore: vec![],
            active_conversation_id: None,
            is_conversation_only: true,
        }),
    })
}

/// A conversation pane (no process behind its `TerminalView`) must not be resurrected as
/// a real, shell-spawning `PaneTemplate` when a session containing one is saved as a
/// launch config -- see `docs/design/moth-parliament.md` step 1 and step 5's warning
/// against resurrecting a shell nobody asked for. It should be dropped exactly like the
/// already-unsupported `Notebook` pane in `test_config_from_snapshot_filters_panes` above.
///
/// Without the `terminal.is_conversation_only` guard in `PaneTemplateType`'s
/// `TryFrom<PaneNodeSnapshot>`, this would instead assert-fail on the `panes.len()`
/// check below: the conversation-only leaf would produce a third `PaneTemplate` with
/// `pane_mode: PaneMode::Terminal`, which spawns a real shell on open.
#[test]
fn test_config_from_snapshot_excludes_conversation_only_pane() {
    let state = single_tab_snapshot(PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Vertical,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![],
                        cwd: Some("/path/to/dir".into()),
                        is_active: true,
                        is_read_only: false,
                        shell_launch_data: None,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        active_conversation_id: None,
                        is_conversation_only: false,
                    }),
                }),
            ),
            (PaneFlex(1.), conversation_only_leaf(false)),
        ],
    }));

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(
        template.windows[0].tabs[0].layout,
        // Flattened to a single leaf: same collapse `test_config_from_snapshot_flattens_single_pane`
        // exercises for a single surviving pane out of a branch.
        PaneTemplateType::PaneTemplate {
            is_focused: Some(true),
            cwd: PathBuf::from("/path/to/dir"),
            commands: vec![],
            pane_mode: PaneMode::Terminal,
            shell: None,
        },
    )
}

/// If a tab's *only* pane is a conversation pane, the whole tab is dropped, the same way
/// `test_config_from_snapshot_filters_tabs` drops a tab whose only pane is unsupported.
///
/// Without the guard, this would instead assert-fail: the tab would survive with a
/// `PaneTemplate { pane_mode: PaneMode::Terminal, .. }` root, spawning a live shell where
/// the user had a conversation with none the moment this launch config is opened.
#[test]
fn test_config_from_snapshot_excludes_conversation_only_tab_entirely() {
    let state = single_tab_snapshot(conversation_only_leaf(true));

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert!(template.windows[0].tabs.is_empty())
}

#[test]
fn test_tab_level_commands_are_applied_to_leaf_layout() {
    let config: LaunchConfig = serde_yaml::from_str(
        r#"
name: Legacy Commands
windows:
  - tabs:
      - layout:
          cwd: /tmp
        commands:
          - exec: echo hello
"#,
    )
    .expect("launch config should parse");

    let layout = config.windows[0].tabs[0].layout_with_tab_commands();

    assert_eq!(
        layout,
        PaneTemplateType::PaneTemplate {
            cwd: PathBuf::from("/tmp"),
            commands: vec![CommandTemplate {
                exec: "echo hello".to_string()
            }],
            is_focused: None,
            pane_mode: PaneMode::Terminal,
            shell: None,
        }
    );
}

#[test]
fn test_tab_level_commands_are_applied_to_focused_pane_in_branch_layout() {
    let config: LaunchConfig = serde_yaml::from_str(
        r#"
name: Legacy Commands
windows:
  - tabs:
      - layout:
          split_direction: horizontal
          panes:
            - cwd: /tmp/left
              is_focused: false
            - cwd: /tmp/right
              is_focused: true
        commands:
          - exec: echo focused
"#,
    )
    .expect("launch config should parse");

    let layout = config.windows[0].tabs[0].layout_with_tab_commands();

    assert_eq!(
        layout,
        PaneTemplateType::PaneBranchTemplate {
            split_direction: SplitDirection::Horizontal.into(),
            panes: vec![
                PaneTemplateType::PaneTemplate {
                    cwd: PathBuf::from("/tmp/left"),
                    commands: vec![],
                    is_focused: Some(false),
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
                PaneTemplateType::PaneTemplate {
                    cwd: PathBuf::from("/tmp/right"),
                    commands: vec![CommandTemplate {
                        exec: "echo focused".to_string()
                    }],
                    is_focused: Some(true),
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
            ],
        }
    );
}

#[test]
fn test_tab_level_commands_are_applied_to_first_pane_without_focused_pane() {
    let config: LaunchConfig = serde_yaml::from_str(
        r#"
name: Legacy Commands
windows:
  - tabs:
      - layout:
          split_direction: horizontal
          panes:
            - cwd: /tmp/left
            - cwd: /tmp/right
        commands:
          - exec: echo first
"#,
    )
    .expect("launch config should parse");

    let layout = config.windows[0].tabs[0].layout_with_tab_commands();

    assert_eq!(
        layout,
        PaneTemplateType::PaneBranchTemplate {
            split_direction: SplitDirection::Horizontal.into(),
            panes: vec![
                PaneTemplateType::PaneTemplate {
                    cwd: PathBuf::from("/tmp/left"),
                    commands: vec![CommandTemplate {
                        exec: "echo first".to_string()
                    }],
                    is_focused: None,
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
                PaneTemplateType::PaneTemplate {
                    cwd: PathBuf::from("/tmp/right"),
                    commands: vec![],
                    is_focused: None,
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
            ],
        }
    );
}

#[test]
fn test_config_with_active_tab_index() {
    let state = multi_tab_snapshot(
        1,
        vec![
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                group_id: None,
                pinned: false,
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                                uuid: vec![],
                                cwd: Some("/path/to/dir".into()),
                                is_active: true,
                                is_read_only: false,
                                shell_launch_data: None,
                                input_config: None,
                                llm_model_override: None,
                                active_profile_id: None,
                                conversation_ids_to_restore: vec![],
                                active_conversation_id: None,
                                is_conversation_only: false,
                            }),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None
            };
            3
        ],
    );

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(template.windows[0].active_tab_index, Some(1))
}

#[test]
fn test_config_with_active_tab_index_and_filtered_tabs() {
    let state = multi_tab_snapshot(
        1,
        vec![
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                group_id: None,
                pinned: false,
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Notebook(
                                NotebookPaneSnapshot::NotebookObject {
                                    notebook_id: None,
                                    settings: ZapDriveObjectSettings::default(),
                                },
                            ),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None,
            },
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                group_id: None,
                pinned: false,
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                                uuid: vec![],
                                cwd: Some("/path/to/dir".into()),
                                is_active: true,
                                is_read_only: false,
                                shell_launch_data: None,
                                input_config: None,
                                llm_model_override: None,
                                active_profile_id: None,
                                conversation_ids_to_restore: vec![],
                                active_conversation_id: None,
                                is_conversation_only: false,
                            }),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None,
            },
        ],
    );

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(template.windows[0].active_tab_index, Some(0))
}

#[test]
fn test_config_with_active_tab_being_filtered() {
    let state = multi_tab_snapshot(
        1,
        vec![
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                group_id: None,
                pinned: false,
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                                uuid: vec![],
                                cwd: Some("/path/to/dir".into()),
                                is_active: true,
                                is_read_only: false,
                                shell_launch_data: None,
                                input_config: None,
                                llm_model_override: None,
                                active_profile_id: None,
                                conversation_ids_to_restore: vec![],
                                active_conversation_id: None,
                                is_conversation_only: false,
                            }),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None,
            },
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                group_id: None,
                pinned: false,
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Notebook(
                                NotebookPaneSnapshot::NotebookObject {
                                    notebook_id: None,
                                    settings: ZapDriveObjectSettings::default(),
                                },
                            ),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None,
            },
        ],
    );

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(template.windows[0].active_tab_index, None)
}
