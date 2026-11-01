use super::*;

#[test]
fn editing_keys_are_held_and_everything_else_acts_at_once() {
    assert!(matches!(
        held_form(&EditorAction::Enter),
        Some(HeldPasteKey::Action(_))
    ));
    assert!(matches!(
        held_form(&EditorAction::Backspace),
        Some(HeldPasteKey::Action(_))
    ));
    assert!(matches!(
        held_form(&EditorAction::Paste),
        Some(HeldPasteKey::Action(_))
    ));
    assert!(matches!(
        held_form(&EditorAction::UserInsert(warp_util::user_input::UserInput::new("q"))),
        Some(HeldPasteKey::Text(text)) if text == "q"
    ));
    // Ctrl-C cancels the paste and Escape/Copy have nothing to order against it.
    assert!(held_form(&EditorAction::CtrlC).is_none());
    assert!(held_form(&EditorAction::Escape).is_none());
    assert!(held_form(&EditorAction::Copy).is_none());
}

#[test]
fn held_keys_come_back_in_typing_order_and_a_superseded_paste_keeps_them() {
    let hold = EditorPasteHold::default();
    assert!(!hold.hold(HeldPasteKey::Text("lost?".into())));
    let first = hold.begin();
    assert!(hold.hold(HeldPasteKey::Text("1".into())));
    let second = hold.begin();
    assert!(hold.hold(HeldPasteKey::Action(EditorAction::Enter)));
    assert!(hold.complete(first).is_none());
    let held = hold.complete(second).expect("second paste is pending");
    assert!(matches!(
        held.as_slice(),
        [HeldPasteKey::Text(text), HeldPasteKey::Action(EditorAction::Enter)] if text == "1"
    ));
    assert!(!hold.has_pending());
}
