use super::*;

#[test]
fn test_has_horizontal_split() {
    let single_leaf = PaneNodeSnapshot::Leaf(LeafSnapshot {
        is_focused: false,
        custom_vertical_tabs_title: None,
        contents: LeafContents::Code(CodePaneSnapShot::Local {
            tabs: vec![CodePaneTabSnapshot {
                path: Some(PathBuf::new()),
            }],
            active_tab_index: 0,
            source: None,
        }),
    });
    assert!(!single_leaf.has_horizontal_split());

    let horizontal_split = PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Horizontal,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Code(CodePaneSnapShot::Local {
                        tabs: vec![CodePaneTabSnapshot {
                            path: Some(PathBuf::new()),
                        }],
                        active_tab_index: 0,
                        source: None,
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Code(CodePaneSnapShot::Local {
                        tabs: vec![CodePaneTabSnapshot {
                            path: Some(PathBuf::new()),
                        }],
                        active_tab_index: 0,
                        source: None,
                    }),
                }),
            ),
        ],
    });
    assert!(horizontal_split.has_horizontal_split());
}

/// A conversation pane's snapshot (`docs/design/moth-parliament.md` step 1) is skipped
/// during `save_app_state`'s pane-tree traversal, the same way `LeafContents::Image` is: it
/// renders for the session but is never written to SQLite, so restoring it can never
/// resurrect a real shell the user never asked for (there is currently no
/// `terminal_panes.kind` value for it -- see `TerminalPaneSnapshot::is_conversation_only`'s
/// doc comment for why). This fails if `is_persisted` stops reading the field at all (either
/// direction: a conversation pane would start persisting, or an ordinary terminal pane would
/// stop).
#[test]
fn conversation_pane_snapshot_is_not_persisted() {
    fn terminal_snapshot(is_conversation_only: bool) -> LeafContents {
        LeafContents::Terminal(TerminalPaneSnapshot {
            uuid: vec![],
            cwd: None,
            shell_launch_data: None,
            is_active: true,
            is_read_only: false,
            input_config: None,
            llm_model_override: None,
            active_profile_id: None,
            conversation_ids_to_restore: vec![],
            active_conversation_id: None,
            is_conversation_only,
        })
    }

    assert!(!terminal_snapshot(true).is_persisted());
    assert!(terminal_snapshot(false).is_persisted());
}

#[test]
fn test_code_pane_snapshot_single_tab() {
    let snapshot = CodePaneSnapShot::Local {
        tabs: vec![CodePaneTabSnapshot {
            path: Some(PathBuf::from("/tmp/test.rs")),
        }],
        active_tab_index: 0,
        source: Some(CodeSource::FileTree {
            path: PathBuf::from("/tmp/test.rs"),
        }),
    };
    let CodePaneSnapShot::Local {
        tabs,
        active_tab_index,
        source,
    } = &snapshot;
    assert_eq!(tabs.len(), 1);
    assert_eq!(*active_tab_index, 0);
    assert_eq!(tabs[0].path, Some(PathBuf::from("/tmp/test.rs")));
    assert!(matches!(source, Some(CodeSource::FileTree { .. })));
}

#[test]
fn test_code_pane_snapshot_with_multiple_tabs() {
    let snapshot = CodePaneSnapShot::Local {
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
        source: Some(CodeSource::Link {
            path: PathBuf::from("/tmp/main.rs"),
            range_start: None,
            range_end: None,
        }),
    };
    let CodePaneSnapShot::Local {
        tabs,
        active_tab_index,
        source,
    } = &snapshot;
    assert_eq!(tabs.len(), 3);
    assert_eq!(*active_tab_index, 1);
    assert_eq!(tabs[0].path, Some(PathBuf::from("/tmp/main.rs")));
    assert_eq!(tabs[1].path, Some(PathBuf::from("/tmp/lib.rs")));
    assert_eq!(tabs[2].path, None);
    assert!(matches!(source, Some(CodeSource::Link { .. })));
}

/// `active_window_index` must count only the windows that survive filtering.
///
/// This is the case that distinguishes the two possible readings of the field:
/// with nothing filtered out, an index into the unfiltered window list and an
/// index into `AppState::windows` are the same number, and the bug is
/// invisible. Every window dropped ahead of the active one shifts them apart.
#[test]
fn test_active_window_index_counts_only_persisted_windows() {
    // Window 10 is filtered out (no workspace, a tab-drag preview, or no
    // tabs), so the active window 20 is the *first* entry of the persisted
    // list. Counting over the unfiltered list put it at 1 — `other`.
    let (windows, active) = collect_windows_with_active_index(
        vec![
            (10u32, None),
            (20u32, Some("active")),
            (30u32, Some("other")),
        ],
        Some(20),
    );
    assert_eq!(windows, vec!["active", "other"]);
    assert_eq!(active, Some(0));
}

/// With nothing filtered, the index is the plain position.
///
/// This one is the degenerate case, not a regression witness: with no window
/// dropped, an index into the unfiltered list and an index into
/// `AppState::windows` are the same number, so it cannot tell the two readings
/// apart. It is here to pin the common path against an over-correction. (None
/// of these tests can be "run against pre-fix code" in any case — the function
/// they call did not exist before this change; what they discriminate is the
/// two possible *meanings* of the field.)
#[test]
fn test_active_window_index_is_unchanged_when_nothing_is_filtered() {
    let (windows, active) = collect_windows_with_active_index(
        vec![(1u32, Some("first")), (2u32, Some("second"))],
        Some(2),
    );
    assert_eq!(windows, vec!["first", "second"]);
    assert_eq!(active, Some(1));
}

/// Enough dropped windows and the old count ran off the end of the list, so
/// the session restored with no window focused rather than the wrong one.
#[test]
fn test_active_window_index_stays_in_bounds_when_leading_windows_are_filtered() {
    let (windows, active) = collect_windows_with_active_index(
        vec![(1u32, None), (2u32, None), (3u32, Some("only"))],
        Some(3),
    );
    assert_eq!(windows, vec!["only"]);
    // The unfiltered count would have been 2, one past the end of a
    // single-element list.
    assert_eq!(active, Some(0));
    assert!(active.is_some_and(|index| index < windows.len()));
}

/// If the active window is itself filtered out there is no persisted entry to
/// point at, so the index is absent rather than aimed at a neighbour.
#[test]
fn test_active_window_index_is_absent_when_the_active_window_is_filtered() {
    let (windows, active) =
        collect_windows_with_active_index(vec![(1u32, Some("kept")), (2u32, None)], Some(2));
    assert_eq!(windows, vec!["kept"]);
    assert_eq!(active, None);
}

/// No active window at all (nothing focused) stays `None`.
#[test]
fn test_active_window_index_is_absent_when_no_window_is_active() {
    let (windows, active) =
        collect_windows_with_active_index(vec![(1u32, Some("kept"))], None::<u32>);
    assert_eq!(windows, vec!["kept"]);
    assert_eq!(active, None);
}

/// A one-tab window snapshot, `quake_mode` selectable, identifiable by its tab
/// title once converted to a `WindowTemplate`.
///
/// The single pane is a terminal because that is the only `LeafContents`
/// variant `PaneTemplateType::try_from` accepts; anything else converts to
/// `Err(())` and the tab would vanish from the template, making the assertions
/// below pass for the wrong reason.
fn quake_test_window(title: &str, quake_mode: bool) -> WindowSnapshot {
    WindowSnapshot {
        tabs: vec![TabSnapshot {
            custom_title: Some(title.to_owned()),
            root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                is_focused: true,
                custom_vertical_tabs_title: None,
                contents: LeafContents::Terminal(TerminalPaneSnapshot {
                    uuid: vec![],
                    cwd: Some("/some/dir".to_owned()),
                    shell_launch_data: None,
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
        quake_mode,
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

/// `LaunchConfig::from_snapshot` narrows the window list a *second* time, so it
/// must recount rather than copy `AppState::active_window_index`.
///
/// A quake window is only ever created by the hotkey, so any window opened
/// afterwards sits behind one in `window_ids()` order — the arrangement below.
/// `get_app_state` keeps the quake window (it has a workspace and tabs), so the
/// active window's `AppState` index is 1; `from_snapshot` drops it, so the
/// active window is really at 0. Copying the 1 across made
/// `root_view.rs:452-470` match the wrong template on reopen.
///
/// This test lives here rather than in `launch_config_tests.rs` only because
/// both halves of the fix share
/// [`super::collect_windows_with_active_index`] and are being kept together;
/// it is a `launch_configs` test by subject.
#[test]
fn test_launch_config_active_index_counts_only_non_quake_windows() {
    use crate::launch_configs::launch_config::LaunchConfig;

    let app_state = AppState {
        windows: vec![
            quake_test_window("quake", true),
            quake_test_window("active", false),
            quake_test_window("other", false),
        ],
        active_window_index: Some(1),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    let config = LaunchConfig::from_snapshot("Test".to_owned(), &app_state);

    assert_eq!(
        config.windows.len(),
        2,
        "the quake window must not become a template"
    );
    assert_eq!(
        config.active_window_index,
        Some(0),
        "the index must count the templates, not the snapshots"
    );

    let active = config
        .active_window_index
        .and_then(|index| config.windows.get(index))
        .expect("the active window survived the quake filter, so it must be indexable");
    assert_eq!(
        active.tabs[0].title.as_deref(),
        Some("active"),
        "copying the snapshot index would have selected `other`"
    );
}

/// If the active window is itself the quake window there is no template to
/// point at, so the index is absent rather than aimed at a neighbour — the same
/// rule `AppState` follows for its own filters.
#[test]
fn test_launch_config_active_index_is_absent_when_the_active_window_is_quake() {
    use crate::launch_configs::launch_config::LaunchConfig;

    let app_state = AppState {
        windows: vec![
            quake_test_window("normal", false),
            quake_test_window("quake", true),
        ],
        active_window_index: Some(1),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    let config = LaunchConfig::from_snapshot("Test".to_owned(), &app_state);

    assert_eq!(config.windows.len(), 1);
    assert_eq!(config.active_window_index, None);
}
