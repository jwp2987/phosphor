use pathfinder_geometry::{rect::RectF, vector::Vector2F};
use warpui::{
    App, AppContext, WindowId, async_assert, async_assert_eq, integration::AssertionCallback,
};

use crate::features::FeatureFlag;
use crate::integration_testing::view_getters::workspace_view;
use crate::tab::{tab_position_id, uses_vertical_tabs};
use crate::workspace::Workspace;
use crate::workspace::tab_group::TabGroupId;

/// Tolerance, in pixels, for the rect-containment tests below. A member tab is
/// allowed to sit `EDGE_SLACK` outside its container before that counts as
/// "painted outside the group", and a foreign tab has to be `EDGE_SLACK`
/// *inside* the container before that counts as "painted inside the group", so
/// a zero-width or edge-abutting slot never reads as either.
const EDGE_SLACK: f32 = 1.0;

/// The group id of every tab, in tab-bar order.
///
/// Mirrors the filtering `Workspace::tab_bar_slots` applies before it builds
/// layout slots: `FeatureFlag::GroupedTabs` gates grouping entirely, and a
/// `group_id` naming a group that no longer exists renders as ungrouped.
///
/// **This is the model, not the screen.** `tab_bar_slots` is a private method
/// of `workspace::view`, so this reproduces its filtering rather than calling
/// it, and nothing derived from it is evidence that the tab bar painted
/// anything at all. The painted side is [`rendered_tab_bar_faults`]; any
/// assertion that claims something about what is *rendered* has to consult it.
pub fn tab_group_ids(app: &App, window_id: WindowId) -> Vec<Option<TabGroupId>> {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |workspace, _ctx| group_ids_in_tab_order(workspace))
}

/// The group id of every tab, in tab-bar order, read straight off a borrowed
/// workspace. Shared by [`tab_group_ids`] and [`rendered_tab_bar_faults`] so
/// the model view and the rendered check can never disagree about *membership*
/// while they are being compared on *placement*.
fn group_ids_in_tab_order(workspace: &Workspace) -> Vec<Option<TabGroupId>> {
    let grouped_tabs_enabled = FeatureFlag::GroupedTabs.is_enabled();
    workspace
        .tabs
        .iter()
        .map(|tab| {
            if grouped_tabs_enabled {
                tab.group_id
                    .filter(|gid| workspace.tab_groups.contains_key(gid))
            } else {
                None
            }
        })
        .collect()
}

/// One slot of the tab bar the model *asks* for: a maximal run of consecutive
/// tabs that share a group, or a single ungrouped tab.
///
/// This is the decomposition `Workspace::tab_bar_slots` performs to decide what
/// to draw — it collapses only *contiguous* runs — so the number of runs
/// carrying a given group id is the number of group headers the tab bar is
/// supposed to draw. Whether it drew them is a separate question, answered by
/// [`rendered_tab_bar_faults`]. A group split into two runs is the
/// duplicate-header bug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabBarRun {
    /// `None` for a lone ungrouped tab.
    pub group_id: Option<TabGroupId>,
    pub first_index: usize,
    pub len: usize,
}

/// Collapses the tab list into the runs the tab bar is asked to render. See
/// [`TabBarRun`] — this is model state, not rendered output.
pub fn tab_bar_runs(app: &App, window_id: WindowId) -> Vec<TabBarRun> {
    let mut runs: Vec<TabBarRun> = Vec::new();
    for (index, group_id) in tab_group_ids(app, window_id).into_iter().enumerate() {
        match group_id {
            Some(group_id) => match runs.last_mut() {
                Some(run) if run.group_id == Some(group_id) => run.len += 1,
                _ => runs.push(TabBarRun {
                    group_id: Some(group_id),
                    first_index: index,
                    len: 1,
                }),
            },
            None => runs.push(TabBarRun {
                group_id: None,
                first_index: index,
                len: 1,
            }),
        }
    }
    runs
}

/// Labels each tab with the ordinal of its group — groups numbered by order of
/// first appearance in the tab bar — and `None` for an ungrouped tab.
///
/// One pattern that pins membership, tab order and contiguity in the **model**
/// at the same time: a group split in two cannot produce the same labelling as
/// that group intact, because the label of the second run is decided by what
/// came between them. It says nothing about what was painted; the assertions
/// that do are [`assert_groups_contiguous`] and [`assert_group_header_count`].
pub fn tab_group_layout(app: &App, window_id: WindowId) -> Vec<Option<usize>> {
    let mut seen: Vec<TabGroupId> = Vec::new();
    tab_group_ids(app, window_id)
        .into_iter()
        .map(|group_id| {
            let group_id = group_id?;
            let ordinal = match seen.iter().position(|seen_id| *seen_id == group_id) {
                Some(ordinal) => ordinal,
                None => {
                    seen.push(group_id);
                    seen.len() - 1
                }
            };
            Some(ordinal)
        })
        .collect()
}

/// The group the tab at `tab_index` belongs to, if any.
///
/// # Panics
/// If `tab_index` is out of range.
pub fn group_id_for_tab(app: &App, window_id: WindowId, tab_index: usize) -> Option<TabGroupId> {
    *tab_group_ids(app, window_id)
        .get(tab_index)
        .unwrap_or_else(|| panic!("tab {tab_index} should exist for window_id={window_id}"))
}

/// The tab indices belonging to `group_id`, in tab-bar order.
pub fn group_member_indices(app: &App, window_id: WindowId, group_id: TabGroupId) -> Vec<usize> {
    tab_group_ids(app, window_id)
        .into_iter()
        .enumerate()
        .filter_map(|(index, id)| (id == Some(group_id)).then_some(index))
        .collect()
}

/// Number of tab groups defined in this window, whether or not they have
/// members.
pub fn tab_group_count(app: &App, window_id: WindowId) -> usize {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |workspace, _ctx| workspace.tab_groups.len())
}

/// Whether `group_id` is collapsed. Panics if the group does not exist.
pub fn group_is_collapsed(app: &App, window_id: WindowId, group_id: TabGroupId) -> bool {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |workspace, _ctx| {
        workspace
            .tab_groups
            .get(&group_id)
            .unwrap_or_else(|| panic!("group {group_id:?} should exist"))
            .collapsed
    })
}

/// The user-visible name of `group_id`, or `None` while it is still untitled.
/// Panics if the group does not exist.
pub fn group_name(app: &App, window_id: WindowId, group_id: TabGroupId) -> Option<String> {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |workspace, _ctx| {
        workspace
            .tab_groups
            .get(&group_id)
            .unwrap_or_else(|| panic!("group {group_id:?} should exist"))
            .name
            .clone()
    })
}

// ---------------------------------------------------------------------------
// The tab bar the app actually painted
// ---------------------------------------------------------------------------

/// Save-position id the horizontal tab bar writes for a group's container rect.
///
/// Mirrors `htab_group_position_id`, which lives in `workspace::view`'s
/// **private** `mod vertical_tabs`: the function is `pub(crate)` but the module
/// is not, so the path is unreachable from here without widening a module in
/// `workspace/view.rs`. `crates/integration/src/test/tab_groups.rs` rebuilds
/// the same string for the same reason, and
/// `tab_group_save_position_ids_are_distinct_per_axis_and_role`
/// (`workspace/view/vertical_tabs_tests.rs`) pins the real one's prefix.
/// **If that format changes, this must change with it** — a drifted key reads
/// as "the tab bar painted no group", which is a loud failure rather than a
/// silent pass.
fn horizontal_group_position_id(group_id: TabGroupId) -> String {
    format!("horizontal_tabs:group:{group_id:?}")
}

/// Save-position id the vertical tabs panel writes for a group's container
/// rect. Mirrors `vtab_group_position_id`; see
/// [`horizontal_group_position_id`] for why it is rebuilt here.
fn vertical_group_position_id(group_id: TabGroupId) -> String {
    format!("vertical_tabs:group:{group_id:?}")
}

/// The save-position id of `group_id`'s container on whichever tab surface is
/// the one being rendered. `uses_vertical_tabs` is the same predicate the
/// workspace's render uses to pick between the two, so this asks the surface
/// that is actually drawing, not both.
///
/// This is the reader half of `Workspace::group_container_rect`
/// (`workspace/view.rs`), which the shipped drag hit-testing uses to resolve
/// the hovered group: same predicate, same two ids, same last-frame lookup.
/// That function is private, so it is mirrored rather than called.
fn group_position_id(ctx: &AppContext, group_id: TabGroupId) -> String {
    if uses_vertical_tabs(ctx) {
        vertical_group_position_id(group_id)
    } else {
        horizontal_group_position_id(group_id)
    }
}

/// Whether `point` lies inside `rect`, with `slack` pixels of tolerance:
/// positive slack grows the rect (a lenient "is inside" test), negative shrinks
/// it (a strict one, so a slot that merely abuts the container's edge does not
/// read as being inside it).
fn point_within(rect: RectF, point: Vector2F, slack: f32) -> bool {
    point.x() >= rect.min_x() - slack
        && point.x() <= rect.max_x() + slack
        && point.y() >= rect.min_y() - slack
        && point.y() <= rect.max_y() + slack
}

/// The rect the tab surface painted for `group_id`'s container in the last
/// frame, or `None` if it has never painted one.
pub fn rendered_group_container(
    app: &App,
    window_id: WindowId,
    group_id: TabGroupId,
) -> Option<RectF> {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |_workspace, ctx| {
        ctx.element_position_by_id_at_last_frame(window_id, group_position_id(ctx, group_id))
    })
}

/// The rect the tab surface painted for the tab at `tab_index` in the last
/// frame, or `None` if it has never painted one.
pub fn rendered_tab_rect(app: &App, window_id: WindowId, tab_index: usize) -> Option<RectF> {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |_workspace, ctx| {
        ctx.element_position_by_id_at_last_frame(window_id, tab_position_id(tab_index))
    })
}

/// Every group container the tab surface actually painted, for the groups the
/// model says currently have members, ordered by position along the tab bar.
///
/// Groups with no members are excluded: they occupy no layout slot, so the tab
/// bar correctly paints nothing for them.
///
/// **This asks the model which keys to look up.** `PositionCache` offers a
/// keyed lookup and no enumeration, so the only containers this can find are
/// the ones the model already predicts. It sees a container that is *missing*
/// or in the wrong place; it cannot see an *extra* one painted for a group the
/// model no longer holds. With no groups in the model the result is empty
/// whatever the tab bar drew — see [`rendered_tab_bar_faults`] for the full
/// list of what that costs.
pub fn rendered_group_containers(app: &App, window_id: WindowId) -> Vec<(TabGroupId, RectF)> {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |workspace, ctx| {
        let mut painted: Vec<(TabGroupId, RectF)> = groups_with_members(workspace)
            .into_iter()
            .filter_map(|group_id| {
                ctx.element_position_by_id_at_last_frame(
                    window_id,
                    group_position_id(ctx, group_id),
                )
                .map(|rect| (group_id, rect))
            })
            .collect();
        painted.sort_by(|(_, left), (_, right)| {
            left.min_x()
                .total_cmp(&right.min_x())
                .then(left.min_y().total_cmp(&right.min_y()))
        });
        painted
    })
}

/// The groups that have at least one member tab, in order of first appearance
/// in the tab bar.
fn groups_with_members(workspace: &Workspace) -> Vec<TabGroupId> {
    let mut groups: Vec<TabGroupId> = Vec::new();
    for group_id in group_ids_in_tab_order(workspace).into_iter().flatten() {
        if !groups.contains(&group_id) {
            groups.push(group_id);
        }
    }
    groups
}

/// Compares the tab bar the app actually painted in its last frame against the
/// tab group model, returning one message per disagreement. Empty means the
/// paint agrees with the model.
///
/// This is the only place in this module that looks at the screen, and it is
/// what makes [`assert_groups_contiguous`] and [`assert_group_header_count`]
/// able to fail for a *rendering* bug. It reads the same `SavePosition` rects
/// the shipped drag hit-testing reads (`Workspace::tab_bar_slots` →
/// `raw_tab_insertion_index_for_cursor` →
/// `element_position_by_id_at_last_frame`), so a tab bar that painted no group
/// container, painted a group without one of its members, or painted a foreign
/// tab inside a group's container all show up here.
///
/// What it deliberately does not check, and why:
///
/// * **Headers cannot be *counted* from the position cache.** Every run of a
///   group writes its container rect under the same key
///   (`horizontal_tabs:group:<id>`), so a group split across two runs leaves
///   one rect behind however many headers were drawn. The split is caught by
///   geometry instead: the earlier run's members are painted outside the
///   surviving container, and the tab that broke the run is painted inside it.
/// * **Only keys the model predicts are looked up.** `PositionCache` exposes
///   a keyed `get_position` and no way to enumerate what was painted, so every
///   rect here is fetched under an id derived from the model: the ids of the
///   groups the model holds, and the indices of the tabs the model holds.
///   Chrome painted for something the model does *not* have — the second half
///   of the original report, a header with no live group behind it — writes a
///   key nothing here asks for and is invisible. Seeing it needs an
///   enumeration API on `PositionCache` *and* eviction, since a closed group's
///   rect otherwise lingers forever and would itself read as an extra header.
/// * **"This element was not painted this frame" is not observable.**
///   `PositionCache::cache_position_indefinitely` never evicts and
///   `clear_position` is explicit-only, so a rect that has outlived its
///   element is indistinguishable from a fresh one. Every check here is
///   therefore "the rect that exists is in the wrong place", never "no rect
///   exists, so nothing was drawn". The cost is concrete: a regression that
///   stops painting a group's chrome *after* one good frame leaves the last
///   good rect in the cache and passes every lookup below. Only chrome that
///   was never painted at all in this run fails loudly.
/// * **A collapsed group's members** are skipped: a collapsed group paints its
///   container and no member tabs, so those cached rects are stale by
///   construction.
/// * **The members of a group that is mid-drag** are skipped, and only they.
///   `SavePosition` is the *outer* wrapper on both a tab (`TabComponent::build`)
///   and a group (`vertical_tabs::render_tab_group_internal`), and it caches
///   `RectF::new(origin, child.size())` **before** it paints its child — so the
///   rect it stores is the layout slot its parent handed it, which a
///   `Draggable` underneath it cannot change by drawing somewhere else. A
///   dragged *tab*'s cached rect is therefore its tab-bar slot and not the
///   floating chip's, so it is checked like any other tab; production reads
///   that same rect mid-drag in `raw_tab_insertion_index_for_cursor`, and the
///   cross-window drag ghost deliberately skips `SavePosition`
///   (`TabComponent::for_drag_ghost`) so it cannot clobber it. A dragged
///   *group*'s container rect is a real slot for the same reason — but its
///   member tabs write their `SavePosition`s from *inside* that `Draggable`,
///   so the overlay paint re-caches each of them at the drag chip's origin.
///   Those member rects are the only ones a drag stops being tab-bar geometry.
/// * **One tab with no painted rect** is not itself a fault: it cannot be told
///   apart from a tab that has simply not been painted yet. What is reported
///   is the tab bar resolving *no* tab rects at all, so a tab bar that painted
///   nothing cannot pass as "no disagreements".
/// * **Grouping turned off** short-circuits: with `FeatureFlag::GroupedTabs`
///   disabled the tab bar draws no group chrome at all, and absent chrome is
///   then correct rather than a fault.
pub fn rendered_tab_bar_faults(app: &App, window_id: WindowId) -> Vec<String> {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |workspace, ctx| {
        let mut faults: Vec<String> = Vec::new();
        if !FeatureFlag::GroupedTabs.is_enabled() {
            return faults;
        }
        let group_ids = group_ids_in_tab_order(workspace);

        // The tabs the model says are on screen right now: everything except
        // the members of a collapsed group, which paint nothing. A single one
        // of these missing its rect is not evidence of anything — it may just
        // not have been painted yet — but *all* of them missing means the tab
        // bar resolved nothing at all, and every per-tab lookup below would
        // then skip silently. Reported once, here, because it is also the only
        // rendered check that runs when the model holds no groups, where
        // otherwise nothing in this function executes at all.
        let expected_on_screen: Vec<usize> = group_ids
            .iter()
            .enumerate()
            .filter_map(|(index, group_id)| {
                let in_collapsed_group = group_id
                    .and_then(|id| workspace.tab_groups.get(&id))
                    .is_some_and(|group| group.collapsed);
                (!in_collapsed_group).then_some(index)
            })
            .collect();
        if !expected_on_screen.is_empty()
            && expected_on_screen.iter().all(|index| {
                ctx.element_position_by_id_at_last_frame(window_id, tab_position_id(*index))
                    .is_none()
            })
        {
            faults.push(format!(
                "the model says tab(s) {expected_on_screen:?} are on screen, but the tab bar has \
                 painted a rect for none of them"
            ));
        }

        for group_id in groups_with_members(workspace) {
            let Some(group) = workspace.tab_groups.get(&group_id) else {
                continue;
            };
            // `SavePosition` wraps the group's `Draggable` and caches the rect
            // before painting, so a dragging group's container is still the
            // layout slot and the foreign-tab check below stays valid. Its
            // members are the exception: their own `SavePosition`s sit *inside*
            // that `Draggable`, so the overlay paint re-caches them at the drag
            // chip's origin.
            let members_follow_the_cursor = group.draggable_state.is_dragging();
            let members: Vec<usize> = group_ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| (*id == Some(group_id)).then_some(index))
                .collect();
            let position_id = group_position_id(ctx, group_id);
            let Some(container) = ctx.element_position_by_id_at_last_frame(window_id, &position_id)
            else {
                faults.push(format!(
                    "group {group_id:?} holds member tab(s) {members:?} in the model, but the \
                     tab bar has painted no container for it (save-position id `{position_id}`)"
                ));
                continue;
            };

            if !group.collapsed && !members_follow_the_cursor {
                for &index in &members {
                    let Some(rect) =
                        ctx.element_position_by_id_at_last_frame(window_id, tab_position_id(index))
                    else {
                        faults.push(format!(
                            "tab {index} is a member of group {group_id:?}, which the tab bar \
                             painted at {container:?}, but no rect was ever painted for the tab \
                             itself"
                        ));
                        continue;
                    };
                    if !point_within(container, rect.center(), EDGE_SLACK) {
                        faults.push(format!(
                            "tab {index} is a member of group {group_id:?} but was painted at \
                             {rect:?}, outside that group's container {container:?}: the group's \
                             members are not one run on screen, so the tab bar draws one header \
                             per run"
                        ));
                    }
                }
            }

            // The other half of the duplicate-header bug: a tab that landed in
            // the middle of a group's run without joining it.
            for (index, member_of) in group_ids.iter().enumerate() {
                if *member_of == Some(group_id) {
                    continue;
                }
                // A tab being dragged is deliberately *not* skipped: its
                // `SavePosition` sits outside its `Draggable`, so its cached
                // rect is the tab-bar slot it currently occupies. A dragged tab
                // painted inside a group's run without having joined the group
                // is the defect this whole module exists for, and mid-drag is
                // the only moment it is visible.
                let other_group = member_of.and_then(|id| workspace.tab_groups.get(&id));
                if other_group.is_some_and(|other| other.collapsed) {
                    // Not painted this frame, so its cached rect is stale.
                    continue;
                }
                if other_group.is_some_and(|other| other.draggable_state.is_dragging()) {
                    // Its group is mid-drag, so this tab re-cached at the drag
                    // chip's origin rather than at a tab-bar slot.
                    continue;
                }
                let Some(rect) =
                    ctx.element_position_by_id_at_last_frame(window_id, tab_position_id(index))
                else {
                    continue;
                };
                if rect.width() <= 0.0 {
                    // A slot collapsed to zero width (the cross-window drag
                    // placeholder) has no interior to be inside anything.
                    continue;
                }
                if point_within(container, rect.center(), -EDGE_SLACK) {
                    faults.push(format!(
                        "tab {index} does not belong to group {group_id:?} but was painted at \
                         {rect:?}, inside that group's container {container:?}: a foreign tab in \
                         the middle of a group's run splits it into two headers"
                    ));
                }
            }
        }
        faults
    })
}

/// Asserts the exact group layout of the tab bar. See [`tab_group_layout`] for
/// how groups are labelled.
///
/// Model state only — it pins membership and order, not rendering.
pub fn assert_tab_group_layout(expected: Vec<Option<usize>>) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let actual = tab_group_layout(app, window_id);
        async_assert_eq!(
            actual,
            expected,
            "tab group layout should be {expected:?} but was {actual:?} \
             (None = ungrouped, Some(n) = nth group by first appearance)"
        )
    })
}

/// Asserts the group-contiguity invariant on both sides: every group's members
/// occupy one unbroken run of tab indices in the model, *and* the tab bar
/// painted every one of them inside that group's single container.
///
/// This is the invariant whose violation produced the duplicate headers.
/// `tab_bar_slots` collapses only contiguous runs, so a group whose members are
/// split by a foreign tab renders one header per run — two headers sharing a
/// single `TabGroupId`, where only the one whose index range matches responds
/// to close. The rendered half comes from [`rendered_tab_bar_faults`], so this
/// fails if the tab bar draws the group wrongly even when the model is intact.
pub fn assert_groups_contiguous() -> AssertionCallback {
    Box::new(move |app, window_id| {
        let runs = tab_bar_runs(app, window_id);
        let mut seen: Vec<TabGroupId> = Vec::new();
        let mut split: Vec<TabGroupId> = Vec::new();
        for run in &runs {
            let Some(group_id) = run.group_id else {
                continue;
            };
            if seen.contains(&group_id) {
                if !split.contains(&group_id) {
                    split.push(group_id);
                }
            } else {
                seen.push(group_id);
            }
        }
        let faults = rendered_tab_bar_faults(app, window_id);
        async_assert!(
            split.is_empty() && faults.is_empty(),
            "every tab group must occupy one contiguous run, so it renders exactly one header. \
             Groups the model splits across runs: {split:?} (tab bar runs were {runs:?}). \
             Disagreements between the painted tab bar and the model: {faults:?}"
        )
    })
}

/// Asserts the model collapses to exactly `expected` grouped runs — one per
/// header the tab bar is asked to draw — and that the tab bar has painted a
/// container for each of those groups, in agreement with the model about where
/// every tab went ([`rendered_tab_bar_faults`]).
///
/// **What the painted half can and cannot see.** [`rendered_group_containers`]
/// looks up one key per group the *model* holds, because `PositionCache` has no
/// enumeration API, so it detects a container that is missing or misplaced and
/// never one that is extra. Two consequences, both load-bearing:
///
/// * The container count alone cannot see a duplicate header — both runs of a
///   split group write the same key — so the split is caught by the geometric
///   disagreements in [`rendered_tab_bar_faults`], not by `painted.len()`.
/// * With `expected == 0` there are no keys to probe, so `painted.len() == 0`
///   holds whatever was drawn. **This assertion cannot see a header painted
///   with no live group behind it**, which is the shape of the "duplicate
///   header that would not close" once its group is gone. At `expected == 0`
///   the only rendered evidence left is `faults`, which at that point checks
///   just that the tab bar painted tab rects at all.
pub fn assert_group_header_count(expected: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let runs = tab_bar_runs(app, window_id);
        let modelled = runs.iter().filter(|run| run.group_id.is_some()).count();
        let painted = rendered_group_containers(app, window_id);
        let faults = rendered_tab_bar_faults(app, window_id);
        async_assert!(
            modelled == expected && painted.len() == expected && faults.is_empty(),
            "tab bar should render {expected} group header(s): the model collapses to {modelled} \
             grouped run(s) (runs were {runs:?}), the app painted {} container(s) {painted:?} for \
             the group(s) the model holds (containers painted for groups the model no longer \
             holds are not observable), and the painted tab bar disagrees with the model in: \
             {faults:?}",
            painted.len()
        )
    })
}

/// Asserts the number of tab groups defined in the window.
pub fn assert_tab_group_count(expected: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let actual = tab_group_count(app, window_id);
        async_assert_eq!(
            actual,
            expected,
            "workspace should have {expected} tab group(s) but had {actual}"
        )
    })
}

/// Asserts the tab at `tab_index` belongs to no group.
pub fn assert_tab_ungrouped(tab_index: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let group_id = group_id_for_tab(app, window_id, tab_index);
        async_assert!(
            group_id.is_none(),
            "tab {tab_index} should be ungrouped but belongs to {group_id:?}"
        )
    })
}

/// Asserts the tab at `tab_index` belongs to the same group as the tab at
/// `other_tab_index`, and that both are in fact grouped.
pub fn assert_tabs_in_same_group(tab_index: usize, other_tab_index: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let group_id = group_id_for_tab(app, window_id, tab_index);
        let other_group_id = group_id_for_tab(app, window_id, other_tab_index);
        if group_id.is_none() {
            return async_assert!(false, "tab {tab_index} should be grouped but is not");
        }
        async_assert_eq!(
            group_id,
            other_group_id,
            "tab {tab_index} ({group_id:?}) should be in the same group as \
             tab {other_tab_index} ({other_group_id:?})"
        )
    })
}

/// Asserts the group containing the tab at `anchor_tab_index` has exactly
/// `expected` members. Panics if that tab is ungrouped.
pub fn assert_group_member_count(anchor_tab_index: usize, expected: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let group_id = group_id_for_tab(app, window_id, anchor_tab_index)
            .unwrap_or_else(|| panic!("tab {anchor_tab_index} should belong to a group"));
        let members = group_member_indices(app, window_id, group_id);
        async_assert_eq!(
            members.len(),
            expected,
            "group of tab {anchor_tab_index} should have {expected} member(s) \
             but had {members:?}"
        )
    })
}

/// Asserts the collapsed state of the group containing the tab at
/// `anchor_tab_index`. Panics if that tab is ungrouped.
///
/// Model state only, deliberately: the visible consequence of collapsing is
/// that the member tabs stop being painted, and "was not painted this frame" is
/// not observable through the position cache — it never evicts, so the members'
/// rects survive the collapse and a stale rect is indistinguishable from a
/// fresh one.
///
/// The rendered side is correspondingly thin, and the earlier wording here
/// overstated it. [`rendered_tab_bar_faults`] checks that *a* container rect
/// exists under the collapsed group's key and that no foreign tab is painted
/// inside it. It does **not** establish that the container was painted *this*
/// frame — a rect cached before the collapse satisfies the lookup forever — and
/// it cannot count containers, so "exactly one" is not among the things
/// checked. A collapse that stopped painting the group's chrome altogether
/// would pass every rendered check in this module.
pub fn assert_group_collapsed(anchor_tab_index: usize, expected: bool) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let group_id = group_id_for_tab(app, window_id, anchor_tab_index)
            .unwrap_or_else(|| panic!("tab {anchor_tab_index} should belong to a group"));
        let collapsed = group_is_collapsed(app, window_id, group_id);
        async_assert_eq!(
            collapsed,
            expected,
            "group of tab {anchor_tab_index} should have collapsed={expected}"
        )
    })
}

/// Asserts the name of the group containing the tab at `anchor_tab_index`.
/// Panics if that tab is ungrouped.
pub fn assert_group_name(
    anchor_tab_index: usize,
    expected: Option<&'static str>,
) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let group_id = group_id_for_tab(app, window_id, anchor_tab_index)
            .unwrap_or_else(|| panic!("tab {anchor_tab_index} should belong to a group"));
        let name = group_name(app, window_id, group_id);
        async_assert_eq!(
            name.as_deref(),
            expected,
            "group of tab {anchor_tab_index} should be named {expected:?} but was {name:?}"
        )
    })
}
