use std::collections::HashMap;

use super::{next_tab_color, tab_group_menu_entry_flags, SelectedTabColor, TabData};
use crate::menu::MenuItem;
use crate::themes::theme::AnsiColorIdentifier;
use crate::ui_components::color_dot::TAB_COLOR_OPTIONS;
use crate::workspace::tab_group::{TabGroup, TabGroupId};
use crate::workspace::WorkspaceAction;

/// Build a `tab_groups` map containing exactly the given group ids.
fn groups(ids: &[TabGroupId]) -> HashMap<TabGroupId, TabGroup> {
    ids.iter()
        .map(|id| {
            let mut group = TabGroup::new();
            group.id = *id;
            (*id, group)
        })
        .collect()
}

// GH-13073: a tab that is the sole member of its group must NOT be offered
// "New group with tab" (it would just recreate an identical single-tab group);
// it offers "Remove from group" instead.
#[test]
fn sole_member_of_group_hides_new_group_and_offers_remove() {
    let gid = TabGroupId::new();
    let (show_new_group, _show_move_to_group, show_remove_from_group) =
        tab_group_menu_entry_flags(Some(gid), &groups(&[gid]), /* is_only_member */ true);

    assert!(
        !show_new_group,
        "the sole member of a group should not offer 'New group with tab'"
    );
    assert!(
        show_remove_from_group,
        "a tab in a group should offer 'Remove from group'"
    );
}

// GH-13073 follow-up: a tab that shares a group with siblings SHOULD still be
// offered "New group with tab" so it can be pulled out into its own new group
// (à la Chrome), and it offers "Remove from group" as well.
#[test]
fn grouped_tab_with_siblings_offers_new_group_and_remove() {
    let gid = TabGroupId::new();
    let (show_new_group, _show_move_to_group, show_remove_from_group) =
        tab_group_menu_entry_flags(Some(gid), &groups(&[gid]), /* is_only_member */ false);

    assert!(
        show_new_group,
        "a grouped tab with siblings should still offer 'New group with tab'"
    );
    assert!(
        show_remove_from_group,
        "a grouped tab should offer 'Remove from group'"
    );
}

// An ungrouped tab always offers "New group with tab" and never offers
// "Remove from group". `is_only_member` is irrelevant when ungrouped.
#[test]
fn ungrouped_tab_offers_new_group_and_hides_remove() {
    let (show_new_group, _show_move_to_group, show_remove_from_group) =
        tab_group_menu_entry_flags(None, &HashMap::new(), /* is_only_member */ false);

    assert!(
        show_new_group,
        "an ungrouped tab should offer 'New group with tab'"
    );
    assert!(
        !show_remove_from_group,
        "an ungrouped tab should not offer 'Remove from group'"
    );
}

// "Move to group" should only appear when a group other than the tab's own
// exists — for both grouped and ungrouped tabs.
#[test]
fn move_to_group_only_shown_when_other_groups_exist() {
    let own = TabGroupId::new();
    let other = TabGroupId::new();

    // Grouped tab whose group is the only one: no other groups to move to.
    let (_n, move_only_own, _r) = tab_group_menu_entry_flags(Some(own), &groups(&[own]), true);
    assert!(!move_only_own);

    // Grouped tab with another group present: offer "Move to group".
    let (_n, move_with_other, _r) =
        tab_group_menu_entry_flags(Some(own), &groups(&[own, other]), true);
    assert!(move_with_other);

    // Ungrouped tab with an existing group: offer "Move to group".
    let (_n, move_ungrouped, _r) = tab_group_menu_entry_flags(None, &groups(&[other]), false);
    assert!(move_ungrouped);
}

#[test]
fn next_tab_color_follows_the_canonical_palette_and_clears_after_the_last_color() {
    assert_eq!(
        next_tab_color(None),
        SelectedTabColor::Color(TAB_COLOR_OPTIONS[0])
    );
    for adjacent_colors in TAB_COLOR_OPTIONS.windows(2) {
        assert_eq!(
            next_tab_color(Some(adjacent_colors[0])),
            SelectedTabColor::Color(adjacent_colors[1])
        );
    }
    let last_color = TAB_COLOR_OPTIONS
        .last()
        .copied()
        .expect("the canonical tab color palette should not be empty");
    assert_eq!(next_tab_color(Some(last_color)), SelectedTabColor::Cleared);
    assert_eq!(
        next_tab_color(SelectedTabColor::Cleared.resolve(None)),
        SelectedTabColor::Color(TAB_COLOR_OPTIONS[0])
    );
    assert_eq!(
        next_tab_color(Some(AnsiColorIdentifier::White)),
        SelectedTabColor::Color(TAB_COLOR_OPTIONS[0])
    );
}

// The metadata-copy entries must never offer a copy that would put an empty
// string on the clipboard: a pane with no resolved branch, pwd or PR reports
// `None`, and a title that is only whitespace is treated the same way.
#[test]
fn copyable_metadata_value_rejects_missing_and_blank_values() {
    assert_eq!(TabData::copyable_metadata_value(None), None);
    assert_eq!(TabData::copyable_metadata_value(Some(String::new())), None);
    assert_eq!(
        TabData::copyable_metadata_value(Some("   \t \n".to_string())),
        None,
        "whitespace-only metadata should be treated as absent"
    );
    assert_eq!(
        TabData::copyable_metadata_value(Some("main".to_string())),
        Some("main".to_string())
    );
    assert_eq!(
        TabData::copyable_metadata_value(Some("  padded  ".to_string())),
        Some("  padded  ".to_string()),
        "a non-blank value is copied verbatim, not trimmed"
    );
}

// A metadata entry with nothing to copy is omitted from the menu entirely,
// rather than rendered as an item that silently does nothing when selected.
#[test]
fn push_copy_metadata_menu_item_skips_entries_with_no_value() {
    let mut menu_items = vec![];
    TabData::push_copy_metadata_menu_item(&mut menu_items, "Copy branch".to_string(), None);

    assert!(
        menu_items.is_empty(),
        "an entry with no value should not be pushed at all"
    );
}

#[test]
fn push_copy_metadata_menu_item_copies_the_value_it_was_given() {
    let mut menu_items = vec![];
    TabData::push_copy_metadata_menu_item(
        &mut menu_items,
        "Copy working directory".to_string(),
        Some("/home/user/project".to_string()),
    );

    assert_eq!(menu_items.len(), 1);
    let MenuItem::Item(fields) = &menu_items[0] else {
        panic!("expected a plain menu item");
    };
    assert_eq!(fields.label(), "Copy working directory");
    assert!(
        matches!(
            fields.on_select_action(),
            Some(WorkspaceAction::CopyTextToClipboard(text)) if text.as_str() == "/home/user/project"
        ),
        "selecting the entry should copy exactly the value it was built with"
    );
}
