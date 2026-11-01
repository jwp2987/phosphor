use std::path::PathBuf;

use super::{ReleasedTab, ReleasedTabs, plan_tab_release};
use crate::code::buffer_location::BufferLocation;

fn local(name: &str) -> Option<BufferLocation> {
    Some(BufferLocation::Local(PathBuf::from(name)))
}

#[test]
fn clean_local_tabs_are_released_keeping_only_locations() {
    let tabs = vec![
        (local("/a.rs"), false, false),
        (local("/b.rs"), true, false),
    ];
    assert_eq!(
        plan_tab_release(&tabs, 1),
        Some(ReleasedTabs {
            tabs: vec![
                ReleasedTab {
                    location: BufferLocation::Local(PathBuf::from("/a.rs")),
                    preview: false,
                },
                ReleasedTab {
                    location: BufferLocation::Local(PathBuf::from("/b.rs")),
                    preview: true,
                },
            ],
            active_tab_index: 1,
        })
    );
}

#[test]
fn unsaved_tab_keeps_every_editor_alive() {
    let tabs = vec![
        (local("/a.rs"), false, false),
        (local("/b.rs"), false, true),
    ];
    assert_eq!(plan_tab_release(&tabs, 0), None);
}

#[test]
fn new_file_and_empty_groups_are_not_released() {
    assert_eq!(plan_tab_release(&[(None, false, false)], 0), None);
    assert_eq!(plan_tab_release(&[], 0), None);
}

#[test]
fn out_of_range_active_index_is_clamped() {
    let released = plan_tab_release(&[(local("/a.rs"), false, false)], 7).unwrap();
    assert_eq!(released.active_tab_index, 0);
}
