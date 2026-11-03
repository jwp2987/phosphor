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

// ---- the ordered rows (route_typed) ----

/// Scripted probes: each answers what it was told and records that it was asked.
#[derive(Default)]
struct Script {
    choice_card: bool,
    interrupt_held: bool,
    setup_holds: bool,
    enter_queue: bool,
    powershell: bool,
    approval: bool,
    settled: bool,
    agent_owns: bool,
    pty_takes: bool,
    input_visible: bool,
    asked: Vec<&'static str>,
}

impl TypedRouteProbes for Script {
    fn ssh_choice_card_up(&mut self) -> bool {
        self.asked.push("choice_card");
        self.choice_card
    }
    fn ssh_interrupt_held(&mut self) -> bool {
        self.asked.push("interrupt_held");
        self.interrupt_held
    }
    fn ssh_interrupt_passes(&mut self) {
        self.asked.push("interrupt_passes");
    }
    fn ssh_setup_holds_key(&mut self) -> bool {
        self.asked.push("setup_holds");
        self.setup_holds
    }
    fn enter_queue_takes_key(&mut self) -> bool {
        self.asked.push("enter_queue");
        self.enter_queue
    }
    fn note_warpification_key(&mut self) {
        self.asked.push("warpification");
    }
    fn powershell_setup_holds(&mut self) -> bool {
        self.asked.push("powershell");
        self.powershell
    }
    fn on_approval_card(&mut self) -> bool {
        self.asked.push("approval");
        self.approval
    }
    fn on_settled_ai_target(&mut self) -> bool {
        self.asked.push("settled");
        self.settled
    }
    fn agent_owns_typing(&mut self) -> bool {
        self.asked.push("agent_owns");
        self.agent_owns
    }
    fn pty_takes_input(&mut self) -> bool {
        self.asked.push("pty_takes");
        self.pty_takes
    }
    fn input_box_visible(&mut self) -> bool {
        self.asked.push("input_visible");
        self.input_visible
    }
}

#[test]
fn ssh_choice_card_row_is_first_and_ends_the_route() {
    let mut s = Script {
        choice_card: true,
        // Every later row would also say yes.
        setup_holds: true,
        enter_queue: true,
        powershell: true,
        approval: true,
        pty_takes: true,
        ..Script::default()
    };
    assert_eq!(route_typed(false, &mut s), TypedRoute::SshChoiceCardHint);
    assert_eq!(s.asked, ["choice_card"]);
}

#[test]
fn interrupt_key_held_ends_the_route_without_asking_the_setup_row() {
    let mut s = Script {
        interrupt_held: true,
        setup_holds: true,
        ..Script::default()
    };
    assert_eq!(route_typed(true, &mut s), TypedRoute::SshInterruptHeld);
    assert_eq!(s.asked, ["choice_card", "interrupt_held"]);
}

#[test]
fn interrupt_key_that_passes_resets_the_typeahead_and_skips_the_setup_row() {
    let mut s = Script::default();
    let route = route_typed(true, &mut s);
    assert!(matches!(route, TypedRoute::Deliver(_)));
    assert_eq!(
        &s.asked[..4],
        [
            "choice_card",
            "interrupt_held",
            "interrupt_passes",
            "enter_queue"
        ]
    );
    assert!(!s.asked.contains(&"setup_holds"));
}

#[test]
fn plain_key_asks_the_setup_row_not_the_interrupt_rows() {
    let mut s = Script {
        setup_holds: true,
        ..Script::default()
    };
    assert_eq!(route_typed(false, &mut s), TypedRoute::SshSetupHeld);
    assert_eq!(s.asked, ["choice_card", "setup_holds"]);
}

#[test]
fn enter_queue_row_comes_before_the_warpification_note_and_powershell() {
    let mut s = Script {
        enter_queue: true,
        powershell: true,
        ..Script::default()
    };
    assert_eq!(route_typed(false, &mut s), TypedRoute::QueuedBehindEnter);
    assert!(!s.asked.contains(&"warpification"));
    assert!(!s.asked.contains(&"powershell"));
}

#[test]
fn powershell_row_holds_whatever_block_is_active() {
    let mut s = Script {
        powershell: true,
        pty_takes: true,
        ..Script::default()
    };
    assert_eq!(route_typed(false, &mut s), TypedRoute::PowerShellHeld);
    // The warpification note ran for the key that got this far.
    assert_eq!(
        s.asked,
        [
            "choice_card",
            "setup_holds",
            "enter_queue",
            "warpification",
            "powershell"
        ]
    );
}

#[test]
fn approval_card_keeps_the_key_from_the_pty_and_from_the_settled_row() {
    let mut s = Script {
        approval: true,
        settled: true,
        pty_takes: true,
        ..Script::default()
    };
    let TypedRoute::Deliver(tail) = route_typed(false, &mut s) else {
        panic!("not delivered");
    };
    assert!(tail.approval_card && !tail.settled_ai_target && !tail.try_pty);
    // The pty is not even asked: the card owns the key.
    assert!(!s.asked.contains(&"settled"));
    assert!(!s.asked.contains(&"pty_takes"));
}

#[test]
fn agent_owned_pty_sends_typing_to_the_agent_input() {
    let mut s = Script {
        agent_owns: true,
        pty_takes: true,
        ..Script::default()
    };
    let TypedRoute::Deliver(tail) = route_typed(false, &mut s) else {
        panic!("not delivered");
    };
    assert!(tail.agent_owns_typing && !tail.try_pty);
    assert!(!s.asked.contains(&"pty_takes"));
}

#[test]
fn pty_that_takes_input_is_tried_and_a_settled_target_is_remembered() {
    let mut s = Script {
        settled: true,
        pty_takes: true,
        ..Script::default()
    };
    let TypedRoute::Deliver(tail) = route_typed(false, &mut s) else {
        panic!("not delivered");
    };
    assert!(tail.try_pty && tail.settled_ai_target);
    assert_eq!(after_pty_attempt(&tail, true, &mut s), TailAction::Done);
}

// ---- after the pty (after_pty_attempt) ----

fn tail(approval: bool, agent: bool, try_pty: bool) -> TypedTail {
    TypedTail {
        approval_card: approval,
        settled_ai_target: false,
        agent_owns_typing: agent,
        try_pty,
    }
}

#[test]
fn pty_refusal_with_hidden_input_drops_the_key() {
    let mut s = Script {
        pty_takes: true,
        input_visible: false,
        ..Script::default()
    };
    assert_eq!(
        after_pty_attempt(&tail(false, false, true), false, &mut s),
        TailAction::DropHiddenInput
    );
}

#[test]
fn pty_refusal_with_visible_input_inserts_into_the_editor() {
    let mut s = Script {
        pty_takes: true,
        input_visible: true,
        ..Script::default()
    };
    assert_eq!(
        after_pty_attempt(&tail(false, false, true), false, &mut s),
        TailAction::InsertIntoEditor
    );
}

#[test]
fn agent_steer_text_is_buffered_not_dropped_even_with_hidden_input() {
    let mut s = Script {
        pty_takes: true,
        input_visible: false,
        ..Script::default()
    };
    assert_eq!(
        after_pty_attempt(&tail(false, true, false), false, &mut s),
        TailAction::InsertIntoEditor
    );
    assert_eq!(
        after_pty_attempt(&tail(true, false, false), false, &mut s),
        TailAction::InsertIntoEditor
    );
}

#[test]
fn pty_that_does_not_take_input_leaves_the_key_to_the_editor() {
    let mut s = Script {
        pty_takes: false,
        input_visible: false,
        ..Script::default()
    };
    assert_eq!(
        after_pty_attempt(&tail(false, false, false), false, &mut s),
        TailAction::InsertIntoEditor
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

    /// The canonical order the rows are asked in.
    const ORDER: [&str; 12] = [
        "choice_card",
        "interrupt_held",
        "interrupt_passes",
        "setup_holds",
        "enter_queue",
        "warpification",
        "powershell",
        "approval",
        "settled",
        "agent_owns",
        "pty_takes",
        "input_visible",
    ];

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// Whatever the answers, the rows are asked in the one order, each at most once, and a
        /// row that ends the route is the last one asked.
        #[test]
        fn rows_are_asked_in_the_one_order(
            interrupt in any::<bool>(),
            bits in vec(any::<bool>(), 11),
        ) {
            let mut s = Script {
                choice_card: bits[0],
                interrupt_held: bits[1],
                setup_holds: bits[2],
                enter_queue: bits[3],
                powershell: bits[4],
                approval: bits[5],
                settled: bits[6],
                agent_owns: bits[7],
                pty_takes: bits[8],
                input_visible: bits[9],
                ..Script::default()
            };
            let route = route_typed(interrupt, &mut s);
            if let TypedRoute::Deliver(tail) = route {
                let _ = after_pty_attempt(&tail, bits[10], &mut s);
            }
            let positions: Vec<usize> = s
                .asked
                .iter()
                .map(|name| ORDER.iter().position(|o| o == name).expect("known row"))
                .collect();
            prop_assert!(
                positions.windows(2).all(|w| w[0] <= w[1]),
                "asked out of order: {:?}",
                s.asked
            );
            for name in ORDER {
                // The pty question is asked again after a refused write, nothing else twice.
                let limit = if name == "pty_takes" { 2 } else { 1 };
                let count = s.asked.iter().filter(|asked| **asked == name).count();
                prop_assert!(count <= limit, "{name} asked {count} times: {:?}", s.asked);
            }
            // A row that ends the route is the last one asked.
            let ended_on = match route {
                TypedRoute::SshChoiceCardHint => Some("choice_card"),
                TypedRoute::SshInterruptHeld => Some("interrupt_held"),
                TypedRoute::SshSetupHeld => Some("setup_holds"),
                TypedRoute::QueuedBehindEnter => Some("enter_queue"),
                TypedRoute::PowerShellHeld => Some("powershell"),
                TypedRoute::Deliver(_) => None,
            };
            if let Some(row) = ended_on {
                prop_assert_eq!(s.asked.last().copied(), Some(row));
            }
        }
    }
}
