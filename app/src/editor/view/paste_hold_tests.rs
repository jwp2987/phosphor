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
    // This pins the `PasteSequencer` path plain editors (search boxes, notebooks) use.
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
    assert!(!hold.sequencer_pending());
}

/// #1389: keys that read the last frame's soft-wrap layout wait for a layout newer than the edit
/// before them; keys that only read the display map do not.
#[test]
fn only_visual_line_keys_read_the_soft_wrap_layout() {
    for action in [
        EditorAction::Up,
        EditorAction::Down,
        EditorAction::SelectUp,
        EditorAction::SelectDown,
        EditorAction::MoveToVisualLineStart,
        EditorAction::MoveToVisualLineEnd,
    ] {
        assert!(HeldPasteKey::Action(action).reads_soft_wrap_layout());
    }
    for key in [
        HeldPasteKey::Action(EditorAction::Left),
        HeldPasteKey::Action(EditorAction::Backspace),
        HeldPasteKey::Action(EditorAction::MoveToLineEnd),
        HeldPasteKey::Text("x".into()),
    ] {
        assert!(!key.reads_soft_wrap_layout());
    }
}

/// Each stored layout is a new generation and wakes the replay step.
#[test]
fn a_stored_layout_bumps_the_generation_and_wakes_the_waiter() {
    use crate::editor::soft_wrap::{FrameLayouts, SoftWrapState};
    let state = SoftWrapState::default();
    assert!(!state.is_laid_out());
    assert_eq!(state.generation(), 0);
    let (tx, rx) = async_channel::bounded::<()>(1);
    state.set_layout_waker(tx);
    state.update(FrameLayouts::new(Vec::new(), 0, 0));
    assert!(state.is_laid_out());
    assert_eq!(state.generation(), 1);
    assert!(rx.try_recv().is_ok(), "the waiter is told a layout landed");
}
