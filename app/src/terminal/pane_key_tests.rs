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
fn control_keys_go_to_the_terminal_only_when_it_has_focus_or_the_hold_needs_it() {
    let terminal = PaneFocusFacts {
        terminal_focused: true,
        ..facts()
    };
    assert_eq!(
        key_target(KeyClass::Control, &terminal),
        KeyTarget::Terminal
    );
    // The editor's control keys are its keybindings.
    let editor = PaneFocusFacts {
        editor_focused: true,
        ssh_hold_active: true,
        ..facts()
    };
    assert_eq!(key_target(KeyClass::Control, &editor), KeyTarget::NotMine);
    // A focus gap during the hold (#1332/#1394).
    let gap = PaneFocusFacts {
        ssh_hold_active: true,
        ..facts()
    };
    assert_eq!(key_target(KeyClass::Control, &gap), KeyTarget::Terminal);
    // A card without a hold: its own bindings, as before.
    assert_eq!(key_target(KeyClass::Control, &facts()), KeyTarget::NotMine);
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
