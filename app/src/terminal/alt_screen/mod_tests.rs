use super::*;

#[test]
fn sgr_mouse_alt_screen_still_scans_for_file_paths() {
    // Claude Code and other TUIs enable SGR mouse; paths they print must still be detected.
    assert!(!skip_file_link_scan(true, true));
    assert!(!skip_file_link_scan(false, true));
}

#[test]
fn kill_switch_restores_editor_behaviour() {
    assert!(skip_file_link_scan(true, false));
    assert!(!skip_file_link_scan(false, false));
}

#[test]
fn link_menu_needs_the_open_link_modifier() {
    let none = warpui::event::ModifiersState::default();
    assert!(!link_menu_modifier_held(&none));
    let held = warpui::event::ModifiersState {
        cmd: true,
        ctrl: true,
        ..Default::default()
    };
    assert!(link_menu_modifier_held(&held));
}
