use winit::keyboard::ModifiersState;

use super::*;

fn state() -> WindowState {
    WindowState::new(crate::WindowId::new())
}

/// #1443: a Ctrl whose key-up went to another window must not survive the focus change, or every
/// later key is a chord.
#[test]
fn losing_focus_forgets_a_held_modifier() {
    let mut ws = state();
    ws.modifiers = ModifiersState::CONTROL | ModifiersState::SUPER;
    assert!(ws.reset_modifiers_for_focus_change());
    assert!(ws.modifiers.is_empty());
}

#[test]
fn regaining_focus_forgets_a_modifier_released_meanwhile() {
    let mut ws = state();
    ws.modifiers = ModifiersState::SHIFT;
    assert!(ws.reset_modifiers_for_focus_change());
    // Reset is idempotent: a second focus event with nothing held reports no change.
    assert!(!ws.reset_modifiers_for_focus_change());
}

#[test]
fn the_per_side_alt_flags_are_cleared_too() {
    let mut ws = state();
    ws.left_alt_pressed = true;
    ws.right_alt_pressed = true;
    assert!(ws.reset_modifiers_for_focus_change());
    assert!(!ws.left_alt_pressed && !ws.right_alt_pressed);
}

#[test]
fn nothing_held_reports_no_change() {
    let mut ws = state();
    assert!(!ws.reset_modifiers_for_focus_change());
}
