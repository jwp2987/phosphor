use warpui::{App, WindowId, async_assert, async_assert_eq, integration::AssertionCallback};

use crate::features::FeatureFlag;
use crate::integration_testing::view_getters::workspace_view;
use crate::workspace::tab_group::TabGroupId;

/// The group id of every tab, in tab-bar order.
///
/// Mirrors the filtering `Workspace::tab_bar_slots` applies before it builds
/// layout slots: `FeatureFlag::GroupedTabs` gates grouping entirely, and a
/// `group_id` naming a group that no longer exists renders as ungrouped.
pub fn tab_group_ids(app: &App, window_id: WindowId) -> Vec<Option<TabGroupId>> {
    let workspace = workspace_view(app, window_id);
    workspace.read(app, |workspace, _ctx| {
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
    })
}

/// One slot of the rendered tab bar: a maximal run of consecutive tabs that
/// share a group, or a single ungrouped tab.
///
/// This is the decomposition `Workspace::tab_bar_slots` performs to decide
/// what to draw — it collapses only *contiguous* runs — so the number of runs
/// carrying a given group id is exactly the number of group headers on
/// screen. A group split into two runs is the duplicate-header bug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabBarRun {
    /// `None` for a lone ungrouped tab.
    pub group_id: Option<TabGroupId>,
    pub first_index: usize,
    pub len: usize,
}

/// Collapses the tab list into the runs the tab bar renders. See [`TabBarRun`].
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
/// One pattern that pins membership, tab order, contiguity and header count at
/// the same time: a group split in two cannot produce the same labelling as
/// that group intact, because the label of the second run is decided by what
/// came between them.
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

/// Asserts the exact group layout of the tab bar. See [`tab_group_layout`] for
/// how groups are labelled.
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

/// Asserts the group-contiguity invariant: every group's members occupy one
/// unbroken run of tab indices.
///
/// This is the invariant whose violation produced the duplicate headers.
/// `tab_bar_slots` collapses only contiguous runs, so a group whose members
/// are split by a foreign tab renders one header per run — two headers
/// sharing a single `TabGroupId`, where only the one whose index range
/// matches responds to close.
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
        async_assert!(
            split.is_empty(),
            "tab groups {split:?} are split across more than one contiguous run, \
             so each renders a duplicate header; tab bar runs were {runs:?}"
        )
    })
}

/// Asserts the tab bar renders exactly `expected` group headers — one per
/// contiguous run of grouped tabs.
pub fn assert_group_header_count(expected: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let runs = tab_bar_runs(app, window_id);
        let headers = runs.iter().filter(|run| run.group_id.is_some()).count();
        async_assert_eq!(
            headers,
            expected,
            "tab bar should render {expected} group header(s) but rendered {headers}; \
             runs were {runs:?}"
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
