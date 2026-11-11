use super::*;

fn facts() -> PaneFocusFacts {
    PaneFocusFacts::default()
}

// ---- who has the key (key_target) ----

#[test]
fn text_field_inside_the_pane_keeps_its_keys() {
    for class in [KeyClass::Typed, KeyClass::Control, KeyClass::Composition] {
        let f = PaneFocusFacts {
            in_text_field: true,
            // Even the ssh hold does not take keys typed into the find bar.
            ssh_hold_active: true,
            ..facts()
        };
        assert_eq!(key_target(class, &f), KeyTarget::NotMine, "{class:?}");
    }
}

#[test]
fn typed_text_goes_to_the_focused_editor() {
    let f = PaneFocusFacts {
        editor_focused: true,
        ..facts()
    };
    assert_eq!(key_target(KeyClass::Typed, &f), KeyTarget::Editor);
}

#[test]
fn typed_text_with_focus_anywhere_else_in_the_pane_is_delivered_not_dropped() {
    // The terminal itself, a card, a settled AI block, a gap: all land on the terminal's routing.
    let f = PaneFocusFacts {
        terminal_focused: true,
        ..facts()
    };
    assert_eq!(key_target(KeyClass::Typed, &f), KeyTarget::Terminal);
    assert_eq!(key_target(KeyClass::Typed, &facts()), KeyTarget::Terminal);
}

#[test]
fn ssh_hold_takes_typed_text_even_when_the_editor_has_focus() {
    // #1400: one ordered hold, offered once, through the terminal.
    let f = PaneFocusFacts {
        editor_focused: true,
        ssh_hold_active: true,
        ..facts()
    };
    assert_eq!(key_target(KeyClass::Typed, &f), KeyTarget::Terminal);
}

#[test]
fn control_keys_follow_focus_the_terminal_and_the_editor_keep_theirs() {
    // #1411: a control key typed while the terminal has focus is translated and routed by the sink,
    // with or without an ssh hold; the editor's are its bindings, hold or not.
    for ssh_hold_active in [false, true] {
        let terminal = PaneFocusFacts {
            terminal_focused: true,
            ssh_hold_active,
            ..facts()
        };
        assert_eq!(
            key_target(KeyClass::Control, &terminal),
            KeyTarget::Terminal
        );
        let editor = PaneFocusFacts {
            editor_focused: true,
            ssh_hold_active,
            settled_ai_focus: true,
            ..facts()
        };
        assert_eq!(key_target(KeyClass::Control, &editor), KeyTarget::NotMine);
    }
    // A focus gap during the hold (#1332/#1394): the sink delivers it.
    let gap = PaneFocusFacts {
        ssh_hold_active: true,
        ..facts()
    };
    assert_eq!(key_target(KeyClass::Control, &gap), KeyTarget::Terminal);
    // #1552: a settled AI block or card has focus: the control key follows the typed keys of the
    // gap through the terminal.
    let settled = PaneFocusFacts {
        settled_ai_focus: true,
        ..facts()
    };
    assert_eq!(key_target(KeyClass::Control, &settled), KeyTarget::Terminal);
    // Fable B1: any other pane child with focus (a pending approval card, a question card, the ssh
    // choice card) binds its own Enter/Tab/Escape, and without a hold the key stays with it.
    assert_eq!(key_target(KeyClass::Control, &facts()), KeyTarget::NotMine);
    // A text field has every key first.
    let text_field = PaneFocusFacts {
        in_text_field: true,
        terminal_focused: true,
        ssh_hold_active: true,
        settled_ai_focus: true,
        ..facts()
    };
    assert_eq!(
        key_target(KeyClass::Control, &text_field),
        KeyTarget::NotMine
    );
}

#[test]
fn control_keys_from_the_input_view_gap_follow_the_typed_keys() {
    // #1552 (a): focus on the input view (or inside it), not its editor, no card: the control key
    // goes through the terminal's routing like the typed keys of the same gap.
    let input_gap = PaneFocusFacts {
        input_gap_focus: true,
        ..facts()
    };
    assert_eq!(
        key_target(KeyClass::Control, &input_gap),
        KeyTarget::Terminal
    );
    assert_eq!(key_target(KeyClass::Typed, &input_gap), KeyTarget::Terminal);
    // The editor focused keeps its bindings; a text field keeps every key.
    let editor = PaneFocusFacts {
        editor_focused: true,
        ..input_gap
    };
    assert_eq!(key_target(KeyClass::Control, &editor), KeyTarget::NotMine);
    let text_field = PaneFocusFacts {
        in_text_field: true,
        ..input_gap
    };
    assert_eq!(
        key_target(KeyClass::Control, &text_field),
        KeyTarget::NotMine
    );
    // Composition is unchanged: shown where it will be committed, never in a gap.
    assert_eq!(
        key_target(KeyClass::Composition, &input_gap),
        KeyTarget::NotMine
    );
    // Without the input gap (any other pane child), Fable B1 still holds.
    assert_eq!(key_target(KeyClass::Control, &facts()), KeyTarget::NotMine);
}

#[test]
fn focus_is_handed_back_at_the_prompt_only_from_a_gap_inside_the_pane() {
    use crate::terminal::input_queue::FocusSnap;
    // #1552 (b)
    assert!(hands_back_focus_at_prompt(FocusSnap::Other, true));
    assert!(!hands_back_focus_at_prompt(FocusSnap::Other, false));
    for kept in [
        FocusSnap::Terminal,
        FocusSnap::Editor,
        FocusSnap::SshChoiceCard,
        FocusSnap::ApprovalCard,
        FocusSnap::AiBlockAwaitingConfirm,
        FocusSnap::SettledAiTarget,
    ] {
        assert!(!hands_back_focus_at_prompt(kept, true), "{kept:?}");
    }
}

#[test]
fn composition_is_shown_where_it_will_be_committed() {
    let editor = PaneFocusFacts {
        editor_focused: true,
        ..facts()
    };
    let terminal = PaneFocusFacts {
        terminal_focused: true,
        ..facts()
    };
    assert_eq!(
        key_target(KeyClass::Composition, &editor),
        KeyTarget::Editor
    );
    assert_eq!(
        key_target(KeyClass::Composition, &terminal),
        KeyTarget::Terminal
    );
    assert_eq!(
        key_target(KeyClass::Composition, &facts()),
        KeyTarget::NotMine
    );
}

// ---- the kill switch ----

#[test]
fn mode_parses_the_environment_value() {
    assert_eq!(PaneKeyMode::parse(None), PaneKeyMode::On);
    assert_eq!(PaneKeyMode::parse(Some("1")), PaneKeyMode::On);
    assert_eq!(PaneKeyMode::parse(Some("0")), PaneKeyMode::Off);
    assert_eq!(PaneKeyMode::parse(Some("off")), PaneKeyMode::Off);
    assert_eq!(PaneKeyMode::parse(Some("shadow")), PaneKeyMode::Shadow);
}

// ---- model-based: every key is delivered exactly once, in arrival order ----

mod model {
    use super::*;
    use proptest::collection::vec;
    use proptest::prelude::{ProptestConfig, Strategy, any, proptest};
    use proptest::{prop_assert, prop_assert_eq};

    /// One step of the world: a key arrives with these facts.
    #[derive(Clone, Copy, Debug)]
    struct Step {
        in_text_field: bool,
        editor_focused: bool,
        terminal_focused: bool,
        ssh_hold_active: bool,
    }

    fn step() -> impl Strategy<Value = Step> {
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
            |(in_text_field, editor_focused, terminal_focused, ssh_hold_active)| Step {
                in_text_field,
                editor_focused,
                terminal_focused,
                ssh_hold_active,
            },
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// Focus moves, ssh holds opening and closing, keys arriving in between: every key is
        /// delivered to exactly one destination (or left to a text field that owns it), and the
        /// keys of a hold all go to the ONE ordered stream, never split between two.
        #[test]
        fn every_typed_key_is_delivered_exactly_once(steps in vec(step(), 0..64)) {
            let mut editor = Vec::new();
            let mut terminal = Vec::new();
            let mut not_mine = Vec::new();
            for (seq, s) in steps.iter().enumerate() {
                let facts = PaneFocusFacts {
                    in_text_field: s.in_text_field,
                    editor_focused: s.editor_focused && !s.in_text_field,
                    terminal_focused: s.terminal_focused,
                    ssh_hold_active: s.ssh_hold_active,
                    settled_ai_focus: false,
                    input_gap_focus: false,
                };
                match key_target(KeyClass::Typed, &facts) {
                    KeyTarget::Editor => editor.push(seq),
                    KeyTarget::Terminal => terminal.push(seq),
                    KeyTarget::NotMine => not_mine.push(seq),
                }
                if s.ssh_hold_active && !s.in_text_field {
                    prop_assert_eq!(terminal.last(), Some(&seq), "a held key joins the hold");
                }
            }
            let mut all: Vec<usize> = editor
                .iter()
                .chain(terminal.iter())
                .chain(not_mine.iter())
                .copied()
                .collect();
            all.sort_unstable();
            prop_assert_eq!(all, (0..steps.len()).collect::<Vec<_>>(), "each key exactly once");
            for seq in &not_mine {
                prop_assert!(steps[*seq].in_text_field, "only a text field's key is left alone");
            }
            // Each destination sees its keys in arrival order.
            prop_assert!(editor.windows(2).all(|w| w[0] < w[1]));
            prop_assert!(terminal.windows(2).all(|w| w[0] < w[1]));
        }
    }
}
