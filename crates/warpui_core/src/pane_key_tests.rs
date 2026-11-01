use super::*;

// One test: the pairing flag is process-wide state, so parallel tests would disturb each other.
#[test]
fn typed_right_after_a_composition_ends_is_its_commit() {
    assert!(!note_event_and_is_ime_commit(false, true), "a plain key");
    assert!(
        !note_event_and_is_ime_commit(true, false),
        "the clear itself"
    );
    assert!(
        note_event_and_is_ime_commit(false, true),
        "typed after the clear is the commit"
    );
    assert!(
        !note_event_and_is_ime_commit(false, true),
        "the commit is one key: the next typed event is a plain key"
    );
    // Another event between the clear and the typed event ends the pairing.
    note_event_and_is_ime_commit(true, false);
    note_event_and_is_ime_commit(false, false);
    assert!(!note_event_and_is_ime_commit(false, true));
}

#[test]
fn context_answers_focus_questions_from_the_chain() {
    let (a, b, c) = (EntityId::new(), EntityId::new(), EntityId::new());
    let ctx = PaneKeyContext {
        focus_chain: vec![a, b],
        leaf_takes_typed_text: false,
    };
    assert_eq!(ctx.leaf(), Some(b));
    assert!(ctx.is_focused_exactly(b));
    assert!(!ctx.is_focused_exactly(a));
    assert!(ctx.in_focus_chain(a) && ctx.in_focus_chain(b));
    assert!(!ctx.in_focus_chain(c));
    assert_eq!(PaneKeyContext::default().leaf(), None);
}
