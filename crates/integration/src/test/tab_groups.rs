//! Integration tests for tab groups.
//!
//! Tab groups carry 151 unit tests, and every one of them passed while three
//! user-reported bugs were live in the shipped app: a tab could not be dragged
//! *out* of a group, a duplicate group header appeared, and the duplicate
//! could not be closed. All three were one defect — `Workspace::on_tab_drag`
//! was missing its group-membership block, so a tab dragged through a group
//! landed inside the group's contiguous run without joining it. `tab_bar_slots`
//! collapses only *contiguous* runs, so a split group renders as two headers
//! sharing one `TabGroupId`, and only the header whose index range matches
//! responds to close.
//!
//! The unit harness cannot reach any of that: `on_tab_drag` resolves the
//! hovered group from laid-out element rects (`element_position_by_id` /
//! `target_group_at_axis`), and a unit test has no laid-out frame to supply.
//! These tests drive real mouse events against the real element tree, so the
//! rect path runs for real, and then assert model state — group membership,
//! tab order, contiguity, header count — rather than pixels.

use pathfinder_geometry::vector::{Vector2F, vec2f};
use warp::cmd_or_ctrl_shift;
use warp::integration_testing::step::new_step_with_default_assertions;
use warp::integration_testing::tab_group::{
    assert_group_collapsed, assert_group_header_count, assert_group_member_count,
    assert_group_name, assert_groups_contiguous, assert_tab_group_count, assert_tab_group_layout,
    assert_tab_ungrouped, assert_tabs_in_same_group, close_tab_group_of_tab,
    create_tab_group_from_tab, ensure_grouped_tabs_enabled, move_tab_to_group_of_tab,
    new_tab_in_group_of_tab, open_settings_file_in_new_tab, rename_tab_group_of_tab,
    toggle_tab_group_collapsed_of_tab,
};
use warp::integration_testing::terminal::wait_until_bootstrapped_single_pane_for_tab;
use warp::integration_testing::workspace::{assert_focused_tab_index, assert_tab_count};
use warpui::{
    WindowId,
    event::{Event, ModifiersState},
    integration::TestStep,
};

use crate::Builder;

use super::new_builder;
use super::workspace::{dispatch_mouse_event, tab_bounds, tab_center};

/// Group name typed by the rename test.
const RENAMED_GROUP: &str = "Deploy";

/// A drag has to move a little before the framework treats it as a drag rather
/// than a click. Small enough that it cannot cross a tab boundary.
const DRAG_ARM_OFFSET: f32 = 12.0;

/// Adds `count` extra terminal tabs (so the workspace ends up with `count + 1`),
/// waiting for each to finish bootstrapping.
fn open_extra_tabs(mut builder: Builder, count: usize) -> Builder {
    for tab_index in 1..=count {
        builder = builder
            .with_step(
                new_step_with_default_assertions(&format!("Open tab {tab_index}"))
                    .with_keystrokes(&[cmd_or_ctrl_shift("t")]),
            )
            .with_step(wait_until_bootstrapped_single_pane_for_tab(tab_index));
    }
    builder
}

/// Presses the left mouse button at the centre of `tab_index` and nudges it far
/// enough to arm the drag, without crossing into a neighbouring slot.
fn begin_tab_drag(tab_index: usize) -> TestStep {
    TestStep::new("Press and arm a tab drag")
        .with_action(move |app, window_id, _| {
            let start = tab_center(app, window_id, tab_index);
            dispatch_mouse_event(
                app,
                window_id,
                Event::LeftMouseDown {
                    position: start,
                    modifiers: ModifiersState::default(),
                    click_count: 1,
                    is_first_mouse: false,
                },
            );
        })
        .with_action(move |app, window_id, _| {
            let start = tab_center(app, window_id, tab_index);
            dispatch_mouse_event(
                app,
                window_id,
                Event::LeftMouseDragged {
                    position: start + vec2f(DRAG_ARM_OFFSET, 0.0),
                    modifiers: ModifiersState::default(),
                },
            );
        })
}

fn drag_to(app: &mut warpui::App, window_id: WindowId, position: Vector2F) {
    dispatch_mouse_event(
        app,
        window_id,
        Event::LeftMouseDragged {
            position,
            modifiers: ModifiersState::default(),
        },
    );
}

/// Drags to the centre of the tab currently occupying `target_tab_index`.
/// The point is recomputed from the live position cache on every event, so a
/// mid-drag reorder does not leave the cursor aiming at a stale slot.
fn drag_over_tab(step_name: &'static str, target_tab_index: usize) -> TestStep {
    TestStep::new(step_name).with_action(move |app, window_id, _| {
        let target = tab_center(app, window_id, target_tab_index);
        drag_to(app, window_id, target);
    })
}

/// Drags onto the leading edge of the tab currently at `target_tab_index`,
/// repeating the event `settle_events` times.
///
/// `calculate_updated_tab_index` moves the dragged tab at most one slot per
/// drag event, so a gesture that crosses several slots needs one event per
/// slot before the tab list stops changing. Extra events past that are no-ops.
fn drag_to_leading_edge_and_settle(
    step_name: &'static str,
    target_tab_index: usize,
    settle_events: usize,
) -> TestStep {
    let mut step = TestStep::new(step_name);
    for _ in 0..settle_events {
        step = step.with_action(move |app, window_id, _| {
            let bounds = tab_bounds(app, window_id, target_tab_index);
            drag_to(
                app,
                window_id,
                vec2f(bounds.min_x() + 5.0, bounds.center().y()),
            );
        });
    }
    step
}

/// Releases the mouse over the leading edge of `target_tab_index`.
fn drop_on_leading_edge(target_tab_index: usize) -> TestStep {
    TestStep::new("Release the drag").with_action(move |app, window_id, _| {
        let bounds = tab_bounds(app, window_id, target_tab_index);
        let target = vec2f(bounds.min_x() + 5.0, bounds.center().y());
        dispatch_mouse_event(
            app,
            window_id,
            Event::LeftMouseUp {
                position: target,
                modifiers: ModifiersState::default(),
            },
        );
    })
}

/// Releases the mouse over the centre of `target_tab_index`.
fn drop_on_tab(target_tab_index: usize) -> TestStep {
    TestStep::new("Release the drag").with_action(move |app, window_id, _| {
        let target = tab_center(app, window_id, target_tab_index);
        dispatch_mouse_event(
            app,
            window_id,
            Event::LeftMouseUp {
                position: target,
                modifiers: ModifiersState::default(),
            },
        );
    })
}

/// Tab dragging is only compiled in with this feature, and every drag test
/// here needs it.
fn drag_tabs_feature_enabled() -> bool {
    cfg!(feature = "drag_tabs_to_windows")
}

/// Four terminal tabs with the last three in one group: `[T0][G: T1 T2 T3]`.
fn four_tabs_with_trailing_group() -> Builder {
    let builder = open_extra_tabs(
        new_builder()
            .set_should_run_test(drag_tabs_feature_enabled)
            .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
            .with_step(ensure_grouped_tabs_enabled()),
        3,
    );
    builder
        .with_step(create_tab_group_from_tab(1))
        .with_step(move_tab_to_group_of_tab(2, 1))
        .with_step(move_tab_to_group_of_tab(3, 1))
        .with_step(
            TestStep::new("Group holds the last three tabs")
                .add_named_assertion(
                    "layout is [ungrouped, group, group, group]",
                    assert_tab_group_layout(vec![None, Some(0), Some(0), Some(0)]),
                )
                .add_named_assertion("one header", assert_group_header_count(1))
                .add_named_assertion("contiguous", assert_groups_contiguous()),
        )
}

/// Four terminal tabs with the last two in one group: `[T0][T1][G: T2 T3]`.
fn four_tabs_with_two_tab_group() -> Builder {
    let builder = open_extra_tabs(
        new_builder()
            .set_should_run_test(drag_tabs_feature_enabled)
            .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
            .with_step(ensure_grouped_tabs_enabled()),
        3,
    );
    builder
        .with_step(create_tab_group_from_tab(2))
        .with_step(move_tab_to_group_of_tab(3, 2))
        .with_step(
            TestStep::new("Group holds the last two tabs")
                .add_named_assertion(
                    "layout is [ungrouped, ungrouped, group, group]",
                    assert_tab_group_layout(vec![None, None, Some(0), Some(0)]),
                )
                .add_named_assertion("one header", assert_group_header_count(1))
                .add_named_assertion("contiguous", assert_groups_contiguous()),
        )
}

/// Dragging a member out of a group clears its `group_id`, and the group keeps
/// the members it still has as one contiguous run under a single header.
///
/// The user-reported bug was that a tab could not be dragged out of a group at
/// all: without the membership block in `on_tab_drag`, `assign_tab_to_group`
/// is never called, so the dragged tab keeps its `group_id` no matter where it
/// is dropped.
pub fn test_drag_tab_out_of_group() -> Builder {
    four_tabs_with_trailing_group()
        .with_step(begin_tab_drag(1))
        // Tab 0 sits to the left of the group's header, i.e. outside the
        // group's accept band, so the drag leaves the group.
        .with_step(drag_over_tab("Drag the first member out to the left", 0))
        .with_step(drop_on_tab(0))
        .with_step(
            TestStep::new("The dragged tab left the group")
                .add_named_assertion("tab count unchanged", assert_tab_count(4))
                .add_named_assertion("dragged tab is ungrouped", assert_tab_ungrouped(1))
                .add_named_assertion(
                    "the group keeps its other two members",
                    assert_group_member_count(2, 2),
                )
                .add_named_assertion(
                    "layout is [ungrouped, ungrouped, group, group]",
                    assert_tab_group_layout(vec![None, None, Some(0), Some(0)]),
                )
                .add_named_assertion("still one header", assert_group_header_count(1))
                .add_named_assertion("still contiguous", assert_groups_contiguous()),
        )
}

/// Dragging an ungrouped tab onto a group makes it join, and it lands inside
/// the group's contiguous block rather than beside it.
///
/// `on_tab_drag` hops the tab to the near edge of the group's index range
/// (`hop_tab_to_index`) precisely so the run stays unbroken; landing it where
/// it happened to be is what produced the second header.
pub fn test_drag_tab_into_group() -> Builder {
    four_tabs_with_two_tab_group()
        .with_step(begin_tab_drag(0))
        // Tab 3 is the group's last member, comfortably inside the group's
        // accept band on the X axis.
        .with_step(drag_over_tab("Drag the first tab onto the group", 3))
        .with_step(drop_on_tab(3))
        .with_step(
            TestStep::new("The dragged tab joined the group")
                .add_named_assertion("tab count unchanged", assert_tab_count(4))
                .add_named_assertion(
                    "the dragged tab is in the group",
                    assert_tabs_in_same_group(1, 2),
                )
                .add_named_assertion(
                    "the group has three members",
                    assert_group_member_count(1, 3),
                )
                .add_named_assertion(
                    "layout is [ungrouped, group, group, group]",
                    assert_tab_group_layout(vec![None, Some(0), Some(0), Some(0)]),
                )
                .add_named_assertion("one header", assert_group_header_count(1))
                .add_named_assertion("contiguous", assert_groups_contiguous()),
        )
}

/// The group-contiguity invariant, asserted directly across a drag that goes
/// over a group and out the other side.
///
/// This is the exact shape of the reported defect. Dragging tab 3 leftwards
/// takes the cursor over the group's members; without the membership block the
/// tab is swapped into the middle of the run while still ungrouped, the run
/// breaks in two, and the tab bar draws a second header carrying the same
/// `TabGroupId` — the duplicate the user saw, which would not close because
/// only the header whose member range matched responded to the action.
///
/// The assertion after the first leg is the regression check: with the fix the
/// tab *joins* the group instead of splitting it. The assertion after the drop
/// pins the settled state once the drag carries on past the group.
pub fn test_drag_through_group_keeps_it_contiguous() -> Builder {
    four_tabs_with_two_tab_group()
        .with_step(begin_tab_drag(1))
        .with_step(drag_over_tab("Drag over the group's last member", 3))
        .with_step(
            TestStep::new("Dragging over the group joins it rather than splitting it")
                .add_named_assertion(
                    "no group is split across two runs",
                    assert_groups_contiguous(),
                )
                .add_named_assertion(
                    "exactly one group header is rendered",
                    assert_group_header_count(1),
                )
                .add_named_assertion(
                    "the dragged tab joined the group",
                    assert_group_member_count(3, 3),
                ),
        )
        // Carry on out the far side. One slot moves per drag event, so send
        // enough events for the tab list to stop changing before the drop.
        .with_step(drag_to_leading_edge_and_settle(
            "Drag on past the group to the front",
            0,
            5,
        ))
        .with_step(drop_on_leading_edge(0))
        .with_step(
            TestStep::new("The group survived the pass as one contiguous run")
                .add_named_assertion("tab count unchanged", assert_tab_count(4))
                .add_named_assertion("no group is split", assert_groups_contiguous())
                .add_named_assertion("exactly one header", assert_group_header_count(1))
                .add_named_assertion("only one group exists", assert_tab_group_count(1)),
        )
}

/// Creating a group from a tab, collapsing and expanding it, and renaming it.
///
/// Collapse/expand and rename both run through the group header, and a
/// collapsed group must still be exactly one slot — `tab_bar_slots` treats it
/// the same as an expanded run, so a collapse that broke the run would show
/// the same duplicate header.
pub fn test_create_collapse_expand_and_rename_tab_group() -> Builder {
    open_extra_tabs(
        new_builder()
            .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
            .with_step(ensure_grouped_tabs_enabled()),
        2,
    )
    .with_step(create_tab_group_from_tab(1))
    .with_step(
        TestStep::new("A new group holds exactly the one tab")
            .add_named_assertion("one group exists", assert_tab_group_count(1))
            .add_named_assertion(
                "layout is [ungrouped, group, ungrouped]",
                assert_tab_group_layout(vec![None, Some(0), None]),
            )
            .add_named_assertion("one header", assert_group_header_count(1))
            .add_named_assertion("the group is expanded", assert_group_collapsed(1, false))
            .add_named_assertion("the group is untitled", assert_group_name(1, None))
            .add_named_assertion("the new member is active", assert_focused_tab_index(1)),
    )
    .with_step(toggle_tab_group_collapsed_of_tab(1))
    .with_step(
        TestStep::new("The group collapses")
            .add_named_assertion("the group is collapsed", assert_group_collapsed(1, true))
            .add_named_assertion("its members are unchanged", assert_group_member_count(1, 1))
            .add_named_assertion(
                "a collapsed group is still one header",
                assert_group_header_count(1),
            )
            .add_named_assertion("still contiguous", assert_groups_contiguous()),
    )
    .with_step(toggle_tab_group_collapsed_of_tab(1))
    .with_step(
        TestStep::new("The group expands again")
            .add_named_assertion("the group is expanded", assert_group_collapsed(1, false))
            .add_named_assertion(
                "layout is unchanged",
                assert_tab_group_layout(vec![None, Some(0), None]),
            )
            .add_named_assertion("one header", assert_group_header_count(1)),
    )
    .with_step(rename_tab_group_of_tab(1))
    .with_step(
        // The rename editor opens focused with the current name selected, so
        // typing replaces it and enter commits.
        TestStep::new("Type the new group name")
            .with_typed_characters(&[RENAMED_GROUP])
            .with_keystrokes(&["enter"])
            .add_named_assertion(
                "the group took the typed name",
                assert_group_name(1, Some(RENAMED_GROUP)),
            )
            .add_named_assertion(
                "renaming did not disturb the layout",
                assert_tab_group_layout(vec![None, Some(0), None]),
            )
            .add_named_assertion("one header", assert_group_header_count(1)),
    )
}

/// Closing a group closes every tab in it and removes the group itself.
///
/// The reported bug's second half was a duplicate header that could not be
/// closed, because `close_tab_group` resolves members by `group_id` while the
/// header the user clicked belonged to the other run. With the group intact,
/// closing it must take exactly its own tabs and nothing else.
pub fn test_close_tab_group_closes_its_tabs() -> Builder {
    open_extra_tabs(
        new_builder()
            .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
            .with_step(ensure_grouped_tabs_enabled()),
        3,
    )
    .with_step(create_tab_group_from_tab(1))
    .with_step(move_tab_to_group_of_tab(2, 1))
    .with_step(
        TestStep::new("Two of the four tabs are grouped")
            .add_named_assertion("four tabs", assert_tab_count(4))
            .add_named_assertion(
                "layout is [ungrouped, group, group, ungrouped]",
                assert_tab_group_layout(vec![None, Some(0), Some(0), None]),
            )
            .add_named_assertion("one header", assert_group_header_count(1)),
    )
    .with_step(close_tab_group_of_tab(1))
    .with_step(
        TestStep::new("Only the group's tabs closed")
            .add_named_assertion("two tabs remain", assert_tab_count(2))
            .add_named_assertion(
                "both survivors are ungrouped",
                assert_tab_group_layout(vec![None, None]),
            )
            .add_named_assertion("no groups remain", assert_tab_group_count(0))
            .add_named_assertion("no headers remain", assert_group_header_count(0)),
    )
}

/// Opening a file in a new tab from inside a group joins that group instead of
/// splitting it, and so does "New tab in group".
///
/// `Workspace::add_tab_for_code_file` takes both its insertion index and its
/// group from `new_tab_index_and_group`. Under the default `AfterCurrentTab`
/// placement the new tab lands immediately after the active tab — which, when
/// the active tab is a group member, is *inside* the group's run. A new tab
/// that landed there without inheriting the group would break the run in two
/// and draw the duplicate header.
pub fn test_open_file_in_new_tab_from_group_joins_group() -> Builder {
    open_extra_tabs(
        new_builder()
            .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
            .with_step(ensure_grouped_tabs_enabled()),
        2,
    )
    .with_step(create_tab_group_from_tab(1))
    .with_step(
        TestStep::new("A one-tab group is active")
            .add_named_assertion("the group member is active", assert_focused_tab_index(1))
            .add_named_assertion(
                "layout is [ungrouped, group, ungrouped]",
                assert_tab_group_layout(vec![None, Some(0), None]),
            ),
    )
    .with_step(open_settings_file_in_new_tab())
    .with_step(
        TestStep::new("The file tab joined the group")
            .add_named_assertion("a tab was added", assert_tab_count(4))
            .add_named_assertion(
                "the file tab is in the same group as its opener",
                assert_tabs_in_same_group(2, 1),
            )
            .add_named_assertion("the group has two members", assert_group_member_count(1, 2))
            .add_named_assertion(
                "layout is [ungrouped, group, group, ungrouped]",
                assert_tab_group_layout(vec![None, Some(0), Some(0), None]),
            )
            .add_named_assertion("one header", assert_group_header_count(1))
            .add_named_assertion("contiguous", assert_groups_contiguous()),
    )
    .with_step(new_tab_in_group_of_tab(1))
    .with_step(
        TestStep::new("\"New tab in group\" also joins the group")
            .add_named_assertion("another tab was added", assert_tab_count(5))
            .add_named_assertion(
                "the group has three members",
                assert_group_member_count(1, 3),
            )
            .add_named_assertion(
                "layout is [ungrouped, group, group, group, ungrouped]",
                assert_tab_group_layout(vec![None, Some(0), Some(0), Some(0), None]),
            )
            .add_named_assertion("one header", assert_group_header_count(1))
            .add_named_assertion("contiguous", assert_groups_contiguous()),
    )
}
