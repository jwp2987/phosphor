use super::*;

#[test]
fn test_split_pane_layout() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];
    let mut root_pane = PaneData::new(panes[0]);

    // Add a pane to the right.
    root_pane.split(panes[0], panes[1], Direction::Right);
    assert_eq!(root_pane.pane_ids(), vec![panes[0], panes[1]]);

    // Insert a vertical (below) pane after the first pane.
    root_pane.split(panes[0], panes[2], Direction::Down);
    assert_eq!(root_pane.pane_ids(), vec![panes[0], panes[2], panes[1]]);

    // Remove the last pane.
    root_pane.remove(panes[1]);
    assert_eq!(root_pane.pane_ids(), vec![panes[0], panes[2]]);

    let panes = [PaneId::dummy_pane_id(); 3];
    let mut root_pane = PaneData::new(panes[0]);

    // Add a pane to the left.
    root_pane.split(panes[0], panes[1], Direction::Left);
    assert_eq!(root_pane.pane_ids(), vec![panes[1], panes[0]]);

    // Add a pane above the first pane.
    root_pane.split(panes[0], panes[2], Direction::Up);
    assert_eq!(root_pane.pane_ids(), vec![panes[2], panes[0], panes[1]]);
}

#[test]
fn test_left_pane_split() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];
    let mut root_pane = PaneData::new(panes[0]);

    root_pane.split(panes[0], panes[1], Direction::Left);
    assert_eq!(root_pane.pane_ids(), vec![panes[1], panes[0]]);

    root_pane.split(panes[0], panes[2], Direction::Left);
    assert_eq!(root_pane.pane_ids(), vec![panes[1], panes[2], panes[0]]);

    root_pane.split(panes[0], panes[3], Direction::Left);
    assert_eq!(
        root_pane.pane_ids(),
        vec![panes[1], panes[2], panes[3], panes[0]]
    );
}

#[test]
fn test_root_split_leaf() {
    let panes = [PaneId::dummy_pane_id(), PaneId::dummy_pane_id()];

    let mut tree = PaneData::new(panes[0]);
    tree.split_root(panes[1], Direction::Down);
    assert_eq!(tree.pane_ids(), vec![panes[0], panes[1]]);
    assert_eq!(
        tree.root.as_branch().expect("Should be a branch").axis(),
        SplitDirection::Vertical
    );
}

#[test]
fn test_root_split_same_axis() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    // Start with a horizontal split.
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);

    // Add a pane at the start of the split.
    tree.split_root(panes[2], Direction::Left);

    // Add a pane at the end of the split.
    tree.split_root(panes[3], Direction::Right);

    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(root.axis(), SplitDirection::Horizontal);
    assert_eq!(
        root.direct_children(),
        vec![panes[2], panes[0], panes[1], panes[3]]
    );
}

#[test]
fn test_root_split_different_axis() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    // Start with a horizontal split:
    // -------------
    // |  0  |  1  |
    // -------------
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);

    // Add a pane above, converting the root to a vertical split:
    // -------------
    // |     2     |
    // -------------
    // |  0  |  1  |
    // -------------
    tree.split_root(panes[2], Direction::Up);
    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(root.axis(), SplitDirection::Vertical);
    assert_eq!(root.node(0).as_leaf(), Some(panes[2]));
    assert_eq!(
        root.node(1)
            .as_branch()
            .expect("Should be a branch")
            .direct_children(),
        vec![panes[0], panes[1]]
    );

    // Add a pane to the right, converting the root to a horizontal split.
    // -------------------
    // |     2     |     |
    // ------------+  3  |
    // |  0  |  1  |     |
    // -------------------
    tree.split_root(panes[3], Direction::Right);
    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(
        root.node(0)
            .as_branch()
            .expect("Should be a branch")
            .get_children(),
        vec![panes[2], panes[0], panes[1]]
    );
    assert_eq!(root.node(1).as_leaf(), Some(panes[3]));
}

#[test]
fn test_move_pane_basic() {
    let panes = [PaneId::dummy_pane_id(), PaneId::dummy_pane_id()];

    // Start with a horizontal split:
    // -------------
    // |  0  |  1  |
    // -------------
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);

    // Move pane 0 to the right of pane 1, which should result in
    // -------------
    // |  1  |  0  |
    // -------------
    tree.move_pane(panes[0], panes[1], Direction::Right);

    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(root.axis(), SplitDirection::Horizontal);
    assert_eq!(root.direct_children(), vec![panes[1], panes[0]]);

    // Move pane 0 on top of pane 1, which should result in
    // --------------
    // |     0      |
    // -------------
    // |     1      |
    // -------------
    tree.move_pane(panes[0], panes[1], Direction::Up);

    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(root.axis(), SplitDirection::Vertical);
    assert_eq!(root.direct_children(), vec![panes[0], panes[1]]);
}

#[test]
fn test_move_pane_multiple_splits() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    // Start with a horizontal split:
    // -------------
    // |  0  |  1  |
    // -------------
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);

    // Add a pane above, converting the root to a vertical split:
    // -------------
    // |     2     |
    // -------------
    // |  0  |  1  |
    // -------------
    tree.split_root(panes[2], Direction::Up);
    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(root.axis(), SplitDirection::Vertical);
    assert_eq!(root.node(0).as_leaf(), Some(panes[2]));
    assert_eq!(
        root.node(1)
            .as_branch()
            .expect("Should be a branch")
            .direct_children(),
        vec![panes[0], panes[1]]
    );

    // Add a pane to the right, converting the root to a horizontal split.
    // -------------------
    // |     2     |     |
    // ------------+  3  |
    // |  0  |  1  |     |
    // -------------------
    tree.split_root(panes[3], Direction::Right);
    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(
        root.node(0)
            .as_branch()
            .expect("Should be a branch")
            .get_children(),
        vec![panes[2], panes[0], panes[1]]
    );
    assert_eq!(root.node(1).as_leaf(), Some(panes[3]));

    // Move Pane 2 to the left of pane 3, which would result in
    // -------------------------
    // |     |     |     |      |
    // | 0   |  1  |  2  |   3  |
    // |     |     |     |      |
    // -------------------------
    tree.move_pane(panes[2], panes[3], Direction::Left);
    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(root.axis(), SplitDirection::Horizontal);
    assert_eq!(
        root.node(0).as_branch().expect("should be branch").axis(),
        SplitDirection::Horizontal
    );
    assert_eq!(
        root.node(0)
            .as_branch()
            .expect("Should be a branch")
            .get_children(),
        vec![panes[0], panes[1]]
    );
    assert_eq!(root.node(1).as_leaf(), Some(panes[2]));
    assert_eq!(root.node(2).as_leaf(), Some(panes[3]));
}

#[test]
fn test_move_pane_no_short_circuit() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    // Setup
    // -------------
    // |     0     |
    // -------------
    // |  1  |  2  |
    // -------------
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Down);
    tree.split(panes[1], panes[2], Direction::Right);

    // Move Pane 1 to the bottom of pane 0.  This should result in a single vertical split
    // with 3 panes, but currently is short circuiting because 1 is already below 0.

    tree.move_pane(panes[1], panes[0], Direction::Down);
    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(root.axis(), SplitDirection::Vertical);
    assert_eq!(root.direct_children(), panes.to_vec());
}

#[test]
fn test_move_pane_no_short_circuit_2() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    // Setup
    // -------------
    // |     0     |
    // -------------
    // |     1     |
    // -------------
    // |     2     |
    // -------------
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Down);
    tree.split(panes[1], panes[2], Direction::Down);

    // Move Pane 1 to the left of pane 2.  This should result in a horizontal split
    // with 2 panes, below pane 0.

    tree.move_pane(panes[1], panes[2], Direction::Left);
    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(root.axis(), SplitDirection::Vertical);
    assert_eq!(root.node(0).as_leaf().expect("Should be a leaf"), panes[0]);
    assert_eq!(
        root.node(1)
            .as_branch()
            .expect("Should be a branch")
            .direct_children(),
        vec![panes[1], panes[2]]
    );
}

#[test]
fn test_sibling_by_direction() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    // Setup
    // -----------------------
    // |         0           |
    // -----------------------
    // |     |     |   3     |
    // |  1  |  2  |---------|
    // |     |     |   4     |
    // -----------------------
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Down);
    tree.split(panes[1], panes[2], Direction::Right);
    tree.split(panes[2], panes[3], Direction::Right);
    tree.split(panes[3], panes[4], Direction::Down);

    assert_eq!(
        tree.sibling_by_direction(panes[1], Direction::Right),
        Some(panes[2])
    );
    assert_eq!(
        tree.sibling_by_direction(panes[2], Direction::Left),
        Some(panes[1])
    );
    assert_eq!(tree.sibling_by_direction(panes[0], Direction::Right), None);
    assert_eq!(tree.sibling_by_direction(panes[0], Direction::Left), None);
    assert_eq!(tree.sibling_by_direction(panes[2], Direction::Right), None);
    assert_eq!(tree.sibling_by_direction(panes[1], Direction::Left), None);
    assert_eq!(tree.sibling_by_direction(panes[1], Direction::Up), None);
    assert_eq!(tree.sibling_by_direction(panes[1], Direction::Down), None);
    assert_eq!(tree.sibling_by_direction(panes[0], Direction::Up), None);
    assert_eq!(tree.sibling_by_direction(panes[0], Direction::Down), None);

    assert_eq!(tree.sibling_by_direction(panes[3], Direction::Up), None);
    assert_eq!(
        tree.sibling_by_direction(panes[3], Direction::Down),
        Some(panes[4])
    );
    assert_eq!(
        tree.sibling_by_direction(panes[4], Direction::Up),
        Some(panes[3])
    );
    assert_eq!(tree.sibling_by_direction(panes[4], Direction::Down), None);
}

#[test]
fn test_pane_by_direction_simple() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);

    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Right),
        FindPaneByDirectionResult::Found(HashSet::from([panes[1]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Left),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Right),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Left),
        FindPaneByDirectionResult::Found(HashSet::from([panes[0]]))
    );

    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Up),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Down),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Up),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Down),
        FindPaneByDirectionResult::Located
    );

    assert_eq!(
        tree.root.panes_by_direction(panes[2], Direction::Right),
        FindPaneByDirectionResult::NotFound
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[2], Direction::Left),
        FindPaneByDirectionResult::NotFound
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[2], Direction::Up),
        FindPaneByDirectionResult::NotFound
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[2], Direction::Down),
        FindPaneByDirectionResult::NotFound
    );
}

#[test]
fn test_pane_by_direction_multi_split() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);
    tree.split(panes[0], panes[2], Direction::Down);
    tree.split(panes[1], panes[3], Direction::Down);

    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Right),
        FindPaneByDirectionResult::Found(HashSet::from([panes[1], panes[3]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Left),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Up),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Down),
        FindPaneByDirectionResult::Found(HashSet::from([panes[2]]))
    );

    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Right),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Left),
        FindPaneByDirectionResult::Found(HashSet::from([panes[0], panes[2]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Up),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Down),
        FindPaneByDirectionResult::Found(HashSet::from([panes[3]]))
    );

    assert_eq!(
        tree.root.panes_by_direction(panes[2], Direction::Right),
        FindPaneByDirectionResult::Found(HashSet::from([panes[1], panes[3]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[2], Direction::Left),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[2], Direction::Up),
        FindPaneByDirectionResult::Found(HashSet::from([panes[0]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[2], Direction::Down),
        FindPaneByDirectionResult::Located
    );

    assert_eq!(
        tree.root.panes_by_direction(panes[3], Direction::Right),
        FindPaneByDirectionResult::Located
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[3], Direction::Left),
        FindPaneByDirectionResult::Found(HashSet::from([panes[0], panes[2]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[3], Direction::Up),
        FindPaneByDirectionResult::Found(HashSet::from([panes[1]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[3], Direction::Down),
        FindPaneByDirectionResult::Located
    );
}

#[test]
fn test_pane_by_direction_multi_level_split() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];

    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[3], Direction::Right);
    tree.split(panes[0], panes[2], Direction::Down);
    tree.split(panes[0], panes[1], Direction::Right);
    tree.split(panes[3], panes[6], Direction::Down);
    tree.split(panes[3], panes[5], Direction::Right);
    tree.split(panes[3], panes[4], Direction::Down);

    assert_eq!(
        tree.root.panes_by_direction(panes[0], Direction::Right),
        FindPaneByDirectionResult::Found(HashSet::from([panes[1]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[1], Direction::Right),
        FindPaneByDirectionResult::Found(HashSet::from([panes[3], panes[4], panes[6]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[5], Direction::Left),
        FindPaneByDirectionResult::Found(HashSet::from([panes[3], panes[4]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[6], Direction::Up),
        FindPaneByDirectionResult::Found(HashSet::from([panes[4], panes[5]]))
    );
    assert_eq!(
        tree.root.panes_by_direction(panes[4], Direction::Down),
        FindPaneByDirectionResult::Found(HashSet::from([panes[6]]))
    );
}

#[test]
fn test_are_rects_overlapping_on_axis() {
    let rect1 = RectF::from_points(Vector2F::new(0.0, 0.0), Vector2F::new(10.0, 10.0));
    let rect2 = RectF::from_points(Vector2F::new(10.0, -5.0), Vector2F::new(20.0, 5.0));
    let rect3 = RectF::from_points(Vector2F::new(10.0, 10.0), Vector2F::new(20.0, 20.0));
    let rect4 = RectF::from_points(Vector2F::new(-5.0, 10.0), Vector2F::new(5.0, 20.0));
    let rect5 = RectF::from_points(Vector2F::new(30.0, 30.0), Vector2F::new(40.0, 40.0));
    let rect6 = RectF::from_points(Vector2F::new(-20.0, -20.0), Vector2F::new(-10.0, -10.0));

    assert!(PaneData::are_rects_overlapping(
        &rect1,
        &rect2,
        SplitDirection::Horizontal
    ));
    assert!(!PaneData::are_rects_overlapping(
        &rect1,
        &rect5,
        SplitDirection::Horizontal
    ));
    assert!(!PaneData::are_rects_overlapping(
        &rect1,
        &rect3,
        SplitDirection::Horizontal
    ));
    assert!(!PaneData::are_rects_overlapping(
        &rect1,
        &rect6,
        SplitDirection::Horizontal
    ));

    assert!(PaneData::are_rects_overlapping(
        &rect1,
        &rect4,
        SplitDirection::Vertical
    ));
    assert!(!PaneData::are_rects_overlapping(
        &rect1,
        &rect5,
        SplitDirection::Vertical
    ),);
    assert!(!PaneData::are_rects_overlapping(
        &rect1,
        &rect3,
        SplitDirection::Vertical
    ));
}

// Ported from the pin (Warp 2026.07.29.09.05, commit 02b53fcd8)
// `app/src/pane_group/tree_tests.rs`, tracked as issue #334. Backs the
// "double-click a divider to reset that split to equal sizes" convenience;
// `reset_pane_sizes` did not exist in the fork before this port.
#[test]
fn test_reset_pane_sizes_resets_containing_branch() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];
    let mut tree = PaneData::new(panes[0]);

    tree.split(panes[0], panes[1], Direction::Right);
    tree.split(panes[1], panes[2], Direction::Right);

    let root = tree.root.as_branch().expect("Should be a branch");
    let border_id = root.dividers[0].id;

    let root = match &mut tree.root {
        PaneNode::Branch(root) => root,
        PaneNode::Leaf(_) => panic!("Should be a branch"),
    };
    root.nodes[0].0 = PaneFlex(0.2);
    root.nodes[1].0 = PaneFlex(0.5);
    root.nodes[2].0 = PaneFlex(0.3);

    assert!(tree.reset_pane_sizes(border_id));

    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(
        root.nodes
            .iter()
            .map(|(flex, _)| flex.0)
            .collect::<Vec<_>>(),
        vec![DEFAULT_FLEX_VALUE, DEFAULT_FLEX_VALUE, DEFAULT_FLEX_VALUE]
    );
}

#[test]
fn test_reset_pane_sizes_only_resets_containing_branch() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];
    let mut tree = PaneData::new(panes[0]);

    tree.split(panes[0], panes[1], Direction::Down);
    tree.split(panes[1], panes[2], Direction::Right);

    let root = match &mut tree.root {
        PaneNode::Branch(root) => root,
        PaneNode::Leaf(_) => panic!("Should be a branch"),
    };
    root.nodes[0].0 = PaneFlex(0.25);
    root.nodes[1].0 = PaneFlex(0.75);

    let nested = match &mut root.nodes[1].1 {
        PaneNode::Branch(nested) => nested,
        PaneNode::Leaf(_) => panic!("Should be a branch"),
    };
    nested.nodes[0].0 = PaneFlex(0.8);
    nested.nodes[1].0 = PaneFlex(0.2);
    let nested_border_id = nested.dividers[0].id;

    assert!(tree.reset_pane_sizes(nested_border_id));

    let root = tree.root.as_branch().expect("Should be a branch");
    assert_eq!(
        root.nodes
            .iter()
            .map(|(flex, _)| flex.0)
            .collect::<Vec<_>>(),
        vec![0.25, 0.75]
    );
    let nested = root.node(1).as_branch().expect("Should be a branch");
    assert_eq!(
        nested
            .nodes
            .iter()
            .map(|(flex, _)| flex.0)
            .collect::<Vec<_>>(),
        vec![DEFAULT_FLEX_VALUE, DEFAULT_FLEX_VALUE]
    );
}

// Issue #334: `reset_pane_sizes` (tested above) had a data layer but no
// caller — nothing translated a double-click on a divider into a call to it,
// so the gesture did nothing for users. These tests cover the wiring added
// to close that gap: `divider_mouse_down_action` (the mouse-down handler
// shared by `create_divider`/`create_minimalist_divider`) must dispatch
// `ResetPaneSizes` on a double-click and `StartResizing` otherwise. Ported
// from the pin's `divider_mouse_down_action` (Warp 2026.07.29.09.05, commit
// 02b53fcd8, `app/src/pane_group/tree.rs`), which has no test of its own —
// this test is new here.
#[test]
fn test_divider_mouse_down_action_double_click_resets_sizes() {
    let mouse_state: MouseStateHandle = Default::default();
    mouse_state
        .lock()
        .unwrap()
        .set_click_count_for_test(Some(2));

    let border_id = EntityId::new();
    let action = divider_mouse_down_action(
        &mouse_state,
        border_id,
        SplitDirection::Horizontal,
        Vector2F::zero(),
    );

    match action {
        PaneGroupAction::ResetPaneSizes(id) => assert_eq!(id, border_id),
        other => panic!("expected ResetPaneSizes, got {other:?}"),
    }
}

#[test]
fn test_divider_mouse_down_action_single_click_starts_resizing() {
    let mouse_state: MouseStateHandle = Default::default();
    mouse_state
        .lock()
        .unwrap()
        .set_click_count_for_test(Some(1));

    let border_id = EntityId::new();
    let action = divider_mouse_down_action(
        &mouse_state,
        border_id,
        SplitDirection::Vertical,
        Vector2F::zero(),
    );

    match action {
        PaneGroupAction::StartResizing(border) => {
            assert_eq!(border.border_id, border_id);
            assert_eq!(border.direction, SplitDirection::Vertical);
        }
        other => panic!("expected StartResizing, got {other:?}"),
    }
}

#[test]
fn test_hide_and_show_child_agent_pane() {
    let panes = [PaneId::dummy_pane_id(), PaneId::dummy_pane_id()];
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);

    // Both panes visible initially.
    assert_eq!(tree.visible_pane_ids(), vec![panes[0], panes[1]]);
    assert!(!tree.is_pane_hidden(&panes[1]));

    // Hide the child agent pane.
    tree.hide_pane_for_child_agent(panes[1]);
    assert!(tree.is_pane_hidden(&panes[1]));
    assert_eq!(tree.visible_pane_ids(), vec![panes[0]]);
    // pane_ids still includes hidden panes (they remain in the tree).
    assert_eq!(tree.pane_ids(), vec![panes[0], panes[1]]);

    // Show the child agent pane.
    tree.show_pane_for_child_agent(panes[1]);
    assert!(!tree.is_pane_hidden(&panes[1]));
    assert_eq!(tree.visible_pane_ids(), vec![panes[0], panes[1]]);
}

#[test]
fn test_hide_child_agent_pane_is_idempotent() {
    let panes = [PaneId::dummy_pane_id(), PaneId::dummy_pane_id()];
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);

    // Hiding the same pane twice should not create duplicate entries.
    tree.hide_pane_for_child_agent(panes[1]);
    tree.hide_pane_for_child_agent(panes[1]);
    assert_eq!(tree.num_hidden_panes(), 1);

    // A single show call should fully unhide it.
    tree.show_pane_for_child_agent(panes[1]);
    assert!(!tree.is_pane_hidden(&panes[1]));
    assert_eq!(tree.num_hidden_panes(), 0);
}

#[test]
fn test_original_pane_for_replacement() {
    let original = PaneId::dummy_pane_id();
    let replacement = PaneId::dummy_pane_id();
    let unrelated = PaneId::dummy_pane_id();
    let mut tree = PaneData::new(original);
    tree.split(original, unrelated, Direction::Right);

    // No replacement yet.
    assert_eq!(tree.original_pane_for_replacement(original), None);
    assert_eq!(tree.original_pane_for_replacement(replacement), None);

    // Perform a temporary replacement.
    assert!(tree.replace_pane(original, replacement, true));
    assert_eq!(
        tree.original_pane_for_replacement(replacement),
        Some(original)
    );
    // The original itself is not a replacement.
    assert_eq!(tree.original_pane_for_replacement(original), None);
    // Unrelated pane is unaffected.
    assert_eq!(tree.original_pane_for_replacement(unrelated), None);

    // Revert — lookup should return None again.
    assert_eq!(
        tree.revert_temporary_replacement(replacement),
        Some(original)
    );
    assert_eq!(tree.original_pane_for_replacement(replacement), None);
}

#[test]
fn test_hide_multiple_child_agent_panes() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[1], Direction::Right);
    tree.split(panes[1], panes[2], Direction::Right);

    tree.hide_pane_for_child_agent(panes[1]);
    tree.hide_pane_for_child_agent(panes[2]);
    assert_eq!(tree.visible_pane_ids(), vec![panes[0]]);

    // Reveal only one child.
    tree.show_pane_for_child_agent(panes[1]);
    assert_eq!(tree.visible_pane_ids(), vec![panes[0], panes[1]]);
    assert!(tree.is_pane_hidden(&panes[2]));
}

/// A divider drag must accumulate across the several events that arrive between two paints.
///
/// `pane_size` reads the position cache, refreshed only by `build_scene`, and paint is deferred
/// from input handling while raw `CursorMoved` events are dispatched uncoalesced -- so N drag
/// events routinely see the same stale measurement. The old code recomputed an absolute flex
/// from that measured size and overwrote the previous result, so only the last event before
/// each repaint survived: the divider moved about 1/N of the cursor's travel, undershooting
/// without ever catching up.
///
/// Non-vacuous, and specifically against that: it drives ten events with `total_size` held
/// constant (the stale case) and asserts the full 100px arrives. The final assertion computes
/// what the old stale-measurement formula would have produced and requires it to differ, so
/// this test cannot pass against the code it replaced.
#[test]
fn test_divider_drag_accumulates_across_events_between_paints() {
    use super::redistributed_flex;

    let total_size = 1000.;
    let total_flex = 2.;
    let start_flex_1 = 1.; // an even 500/500 split
    let delta = 10.;
    let events = 10;

    // Mirrors the two lines in `adjust_pane_size`: derive the divider's position from the flex
    // already applied, then redistribute. `total_size` never changes -- no paint has happened.
    let mut flex_1 = start_flex_1;
    for _ in 0..events {
        let current_size_1 = flex_1 / total_flex * total_size;
        flex_1 = redistributed_flex(current_size_1, delta, total_size, total_flex);
    }

    let moved_px = flex_1 / total_flex * total_size - 500.;
    assert!(
        (moved_px - delta * events as f32).abs() < 0.01,
        "ten 10px drag events between paints must move the divider the full 100px, got {moved_px}"
    );

    // What the replaced code did: every event started from the same stale measured size, so the
    // last one overwrote the rest.
    let stale_size_1 = 500.;
    let old_flex_1 = redistributed_flex(stale_size_1, delta, total_size, total_flex);
    let old_moved_px = old_flex_1 / total_flex * total_size - 500.;
    assert!(
        (old_moved_px - delta).abs() < 0.01,
        "sanity: the old formula should have moved only one event's worth, got {old_moved_px}"
    );
    assert!(
        moved_px > old_moved_px * 2.,
        "the fix must accumulate strictly more travel than the overwrite it replaced"
    );
}

/// The root branch of a tree that has one.
fn root_branch(tree: &PaneData) -> &PaneBranch {
    match &tree.root {
        PaneNode::Branch(branch) => branch,
        PaneNode::Leaf(_) => panic!("the tree has no split"),
    }
}

/// #2144, the layout of the round 15 run: a left pane, editor splits that were opened to its
/// right and closed (hidden for the undo-close window, still in the branch), then a new split
/// to the right. The keyboard resize of the right pane must move the divider between the two
/// panes on screen. It used to pair the right pane with the hidden pane after it, which made
/// "move divider left" shrink the right pane.
#[test]
fn keyboard_resize_pairs_the_focused_pane_with_a_drawn_neighbour() {
    let left = PaneId::dummy_pane_id();
    let closed = [PaneId::dummy_pane_id(), PaneId::dummy_pane_id()];
    let right = PaneId::dummy_pane_id();
    let mut tree = PaneData::new(left);
    for pane in closed {
        tree.split(left, pane, Direction::Right);
        tree.hide_closed_pane(pane);
    }
    tree.split(left, right, Direction::Right);

    let branch = root_branch(&tree);
    // In the branch: left, right, then the closed panes (each split went right of `left`).
    assert_eq!(branch.direct_children()[..2], [left, right]);
    assert_eq!(branch.direct_children().len(), 4);
    let drawn = branch.drawn_node_indices(&tree.hidden_panes);
    assert_eq!(drawn, vec![0, 1]);

    // The right pane (index 1) is the last one drawn: its pair is (left, right).
    assert_eq!(resize_pair_for_child(&drawn, 1), Some((0, 1)));
    // The left pane: the same divider.
    assert_eq!(resize_pair_for_child(&drawn, 0), Some((0, 1)));
    // What the code before #2144 did for the right pane: nodes 1 and 2, the second one hidden.
    let old_pair = (1usize, 2usize);
    assert_ne!(resize_pair_for_child(&drawn, 1), Some(old_pair));
    assert!(!drawn.contains(&old_pair.1));
}

/// #2144: a hidden node BETWEEN two drawn ones. The pair skips it, for the keyboard and for the
/// divider that is drawn between the two (divider 0 on screen).
#[test]
fn resize_pairs_skip_a_hidden_node_between_drawn_ones() {
    let panes = [
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
        PaneId::dummy_pane_id(),
    ];
    let mut tree = PaneData::new(panes[0]);
    tree.split(panes[0], panes[2], Direction::Right);
    tree.split(panes[0], panes[1], Direction::Right);
    tree.hide_pane_for_child_agent(panes[1]);

    let branch = root_branch(&tree);
    assert_eq!(branch.direct_children(), panes.to_vec());
    let drawn = branch.drawn_node_indices(&tree.hidden_panes);
    assert_eq!(drawn, vec![0, 2]);
    assert_eq!(resize_pair_for_child(&drawn, 0), Some((0, 2)));
    assert_eq!(resize_pair_for_child(&drawn, 2), Some((0, 2)));
    // The hidden pane itself has no divider to move.
    assert_eq!(resize_pair_for_child(&drawn, 1), None);
    // The one divider on screen is the first of the branch's dividers.
    assert_eq!(resize_pair_for_divider(&drawn, 0), Some((0, 2)));
    // The branch's second divider is not drawn between anything.
    assert_eq!(resize_pair_for_divider(&drawn, 1), None);
}

/// #2144: with nothing hidden the pairs are the ones of before: a child and its next sibling,
/// the last child and the one before it; divider k between nodes k and k + 1.
#[test]
fn resize_pairs_are_unchanged_when_nothing_is_hidden() {
    let drawn = [0, 1, 2];
    assert_eq!(resize_pair_for_child(&drawn, 0), Some((0, 1)));
    assert_eq!(resize_pair_for_child(&drawn, 1), Some((1, 2)));
    assert_eq!(resize_pair_for_child(&drawn, 2), Some((1, 2)));
    assert_eq!(resize_pair_for_divider(&drawn, 0), Some((0, 1)));
    assert_eq!(resize_pair_for_divider(&drawn, 1), Some((1, 2)));
    assert_eq!(resize_pair_for_divider(&drawn, 2), None);
    // One drawn node: nothing to resize against.
    assert_eq!(resize_pair_for_child(&[3], 3), None);
    assert_eq!(resize_pair_for_child(&[], 0), None);
}

/// #2144: "move divider left" on the right-most pane, in numbers: the pair is (left, right) and
/// a negative delta takes flex from the FIRST of the pair, so the left pane narrows and the
/// right one widens. With the old pair (right, hidden) the same delta took it from the right
/// pane.
#[test]
fn move_divider_left_on_the_right_pane_widens_it() {
    let drawn = [0, 1];
    let (first, second) = resize_pair_for_child(&drawn, 1).expect("two panes on screen");
    assert_eq!((first, second), (0, 1));
    let (total_size, total_flex) = (1352., 2.);
    let mut flex = [1., 1.];
    for _ in 0..2 {
        let current = flex[first] / total_flex * total_size;
        let new_first = redistributed_flex(current, -10., total_size, total_flex);
        flex[second] = total_flex - new_first;
        flex[first] = new_first;
    }
    assert!(
        flex[0] < 1. && flex[1] > 1.,
        "left {} right {}",
        flex[0],
        flex[1]
    );
    let left_px = flex[0] / total_flex * total_size;
    assert!(
        (left_px - (676. - 20.)).abs() < 0.01,
        "left pane {left_px} px"
    );
}

/// #2144: the kill switch; only `0` is off.
#[test]
fn resize_drawn_pair_kill_switch_values() {
    assert!(resize_drawn_pair_enabled_from(None));
    assert!(resize_drawn_pair_enabled_from(Some("1")));
    assert!(resize_drawn_pair_enabled_from(Some("")));
    assert!(!resize_drawn_pair_enabled_from(Some("0")));
    assert!(!resize_drawn_pair_enabled_from(Some(" 0\n")));
}
