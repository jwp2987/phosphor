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

/// With nothing filtered, the index is the plain position — the case that used
/// to pass either way.
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
