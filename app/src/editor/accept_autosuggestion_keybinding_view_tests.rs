use super::close_menu_needs_render;

#[test]
fn an_edit_with_the_menu_closed_does_not_rerender_the_hint() {
    // Every typed character closes the menu; it is almost always closed already.
    assert!(!close_menu_needs_render(false, true));
}

#[test]
fn closing_an_open_menu_rerenders_the_hint() {
    assert!(close_menu_needs_render(true, true));
}

#[test]
fn with_the_kill_switch_every_close_rerenders() {
    assert!(close_menu_needs_render(false, false));
    assert!(close_menu_needs_render(true, false));
}
