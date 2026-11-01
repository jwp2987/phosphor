use super::*;
use crate::keymap::Keystroke;
use crate::{
    App, AppContext, Element, Entity, Event, Presenter, TypedActionView, View, ViewContext,
    elements::Rect, platform::WindowStyle,
};
use std::{cell::RefCell, rc::Rc};

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

// ---- the window dispatch offers each key event to the sink once ----

/// A pane key sink that records what it is offered. It takes typed text and composition, and
/// leaves key downs to the elements (as the terminal view does for printable keys).
struct RecordingSink {
    seen: Rc<RefCell<Vec<PaneKeyKind>>>,
    focus_chain_len: Rc<RefCell<usize>>,
}

impl Entity for RecordingSink {
    type Event = ();
}

impl View for RecordingSink {
    fn ui_name() -> &'static str {
        "pane_key_recording_sink"
    }

    fn render(&self, _: &AppContext) -> Box<dyn Element> {
        Rect::new().finish()
    }

    fn is_pane_key_sink(&self) -> bool {
        true
    }

    fn on_pane_key(
        &mut self,
        key: &PaneKey,
        key_ctx: &PaneKeyContext,
        _: &mut ViewContext<Self>,
    ) -> PaneKeyOutcome {
        self.seen.borrow_mut().push(key.kind.clone());
        *self.focus_chain_len.borrow_mut() = key_ctx.focus_chain.len();
        match key.kind {
            PaneKeyKind::KeyDown { .. } => PaneKeyOutcome::NotMine,
            _ => PaneKeyOutcome::Consumed,
        }
    }
}

impl TypedActionView for RecordingSink {
    type Action = ();
}

/// A printable key down (AltGr is ctrl+alt with a printable character) is not consumed, so the
/// platform follows it with the typed event; the typed event is the key, consumed once. An IME
/// commit is the typed event right after the end of the composition and is marked as one.
#[test]
fn window_dispatch_offers_each_key_event_to_the_sink_once() {
    App::test((), |mut app| async move {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let focus_chain_len = Rc::new(RefCell::new(0));
        let (seen_for_view, len_for_view) = (seen.clone(), focus_chain_len.clone());
        let (window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, move |_| RecordingSink {
                seen: seen_for_view,
                focus_chain_len: len_for_view,
            });
        let presenter = Rc::new(RefCell::new(Presenter::new(window_id)));
        view.update(&mut app, |_, ctx| ctx.focus_self());
        let results = app.update(|ctx| {
            let mut send =
                |event: Event| ctx.simulate_window_event(event, window_id, presenter.clone());
            vec![
                // AltGr + q on a German layout: ctrl+alt, printable text.
                send(Event::KeyDown {
                    keystroke: Keystroke::parse("ctrl-alt-q").expect("valid keystroke"),
                    chars: "@".to_owned(),
                    details: Default::default(),
                    is_composing: false,
                }),
                send(Event::TypedCharacters {
                    chars: "@".to_owned(),
                }),
                send(Event::SetMarkedText {
                    marked_text: "か".to_owned(),
                    selected_range: 0..1,
                }),
                send(Event::ClearMarkedText),
                send(Event::TypedCharacters {
                    chars: "仮".to_owned(),
                }),
                send(Event::TypedCharacters {
                    chars: "x".to_owned(),
                }),
            ]
        });
        assert_eq!(
            results,
            [false, true, true, true, true, true],
            "only the key down is left to the elements (so the typed event follows it)"
        );
        let seen = seen.borrow();
        assert_eq!(seen.len(), 6, "each event offered exactly once: {seen:?}");
        assert!(matches!(
            &seen[0],
            PaneKeyKind::KeyDown { will_type: true, chars, .. } if chars.as_str() == "@"
        ));
        assert!(matches!(
            &seen[1],
            PaneKeyKind::Typed { ime_commit: false, chars } if chars.as_str() == "@"
        ));
        assert!(matches!(&seen[2], PaneKeyKind::SetMarkedText { .. }));
        assert!(matches!(&seen[3], PaneKeyKind::ClearMarkedText));
        assert!(matches!(
            &seen[4],
            PaneKeyKind::Typed { ime_commit: true, chars } if chars.as_str() == "仮"
        ));
        assert!(
            matches!(
                &seen[5],
                PaneKeyKind::Typed {
                    ime_commit: false,
                    ..
                }
            ),
            "the commit is one key"
        );
        assert_eq!(
            *focus_chain_len.borrow(),
            1,
            "the live focus chain is handed over"
        );
    });
}
