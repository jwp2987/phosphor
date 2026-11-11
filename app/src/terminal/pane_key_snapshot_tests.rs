//! Snapshot of the pane key table (#1404): [`key_target`] over every class and focus fact. A
//! change to a row shows up as a reviewed snapshot diff (`cargo insta review`).

use super::*;

macro_rules! snap {
    ($name:literal, $value:expr) => {
        insta::with_settings!({
            prepend_module_to_snapshot => false,
            snapshot_path => "snapshots",
        }, {
            insta::assert_snapshot!($name, $value);
        });
    };
}

#[test]
fn pane_key_target_table() {
    let mut rows = Vec::new();
    for class in [KeyClass::Typed, KeyClass::Control, KeyClass::Composition] {
        for bits in 0..32u8 {
            let facts = PaneFocusFacts {
                in_text_field: bits & 1 != 0,
                editor_focused: bits & 2 != 0,
                terminal_focused: bits & 4 != 0,
                ssh_hold_active: bits & 8 != 0,
                settled_ai_focus: bits & 16 != 0,
                // #1552 (a): its rows are pinned in `pane_key_tests.rs`; the snapshot covers the
                // five facts it always has.
                input_gap_focus: false,
            };
            rows.push(format!(
                "{class:?} text_field={} editor={} terminal={} ssh_hold={} settled_ai={} -> {:?}",
                facts.in_text_field,
                facts.editor_focused,
                facts.terminal_focused,
                facts.ssh_hold_active,
                facts.settled_ai_focus,
                key_target(class, &facts)
            ));
        }
    }
    snap!("pane_key_target_table", rows.join("\n"));
}
