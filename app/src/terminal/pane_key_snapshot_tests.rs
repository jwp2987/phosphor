//! Snapshots of the pane key tables (#1404): [`key_target`] over every class and focus fact, and
//! [`route_typed`] / [`after_pty_attempt`] over named scenarios, with the order the rows asked
//! their questions. A change to a row or to the order shows up as a reviewed snapshot diff
//! (`cargo insta review`).

use std::collections::BTreeSet;

use super::*;

macro_rules! snap {
    ($name:literal, $value:expr) => {
        insta::with_settings!({
            prepend_module_to_snapshot => false,
            snapshot_path => "snapshots",
        }, {
            insta::assert_snapshot!($name, $value);
        });
    };
}

#[test]
fn pane_key_target_table() {
    let mut rows = Vec::new();
    for class in [KeyClass::Typed, KeyClass::Control, KeyClass::Composition] {
        for bits in 0..16u8 {
            let facts = PaneFocusFacts {
                in_text_field: bits & 1 != 0,
                editor_focused: bits & 2 != 0,
                terminal_focused: bits & 4 != 0,
                ssh_hold_active: bits & 8 != 0,
            };
            rows.push(format!(
                "{class:?} text_field={} editor={} terminal={} ssh_hold={} -> {:?}",
                facts.in_text_field,
                facts.editor_focused,
                facts.terminal_focused,
                facts.ssh_hold_active,
                key_target(class, &facts)
            ));
        }
    }
    snap!("pane_key_target_table", rows.join("\n"));
}

/// Answers "yes" to the named questions and records every question asked, in order.
#[derive(Default)]
struct Scripted {
    yes: BTreeSet<&'static str>,
    asked: Vec<&'static str>,
}

impl Scripted {
    fn answering(yes: &[&'static str]) -> Self {
        Self {
            yes: yes.iter().copied().collect(),
            asked: Vec::new(),
        }
    }

    fn ask(&mut self, name: &'static str) -> bool {
        self.asked.push(name);
        self.yes.contains(name)
    }
}

impl TypedRouteProbes for Scripted {
    fn ssh_choice_card_up(&mut self) -> bool {
        self.ask("ssh_choice_card_up")
    }
    fn ssh_interrupt_held(&mut self) -> bool {
        self.ask("ssh_interrupt_held")
    }
    fn ssh_interrupt_passes(&mut self) {
        self.asked.push("ssh_interrupt_passes");
    }
    fn ssh_setup_holds_key(&mut self) -> bool {
        self.ask("ssh_setup_holds_key")
    }
    fn enter_queue_takes_key(&mut self) -> bool {
        self.ask("enter_queue_takes_key")
    }
    fn note_warpification_key(&mut self) {
        self.asked.push("note_warpification_key");
    }
    fn powershell_setup_holds(&mut self) -> bool {
        self.ask("powershell_setup_holds")
    }
    fn on_approval_card(&mut self) -> bool {
        self.ask("on_approval_card")
    }
    fn on_settled_ai_target(&mut self) -> bool {
        self.ask("on_settled_ai_target")
    }
    fn agent_owns_typing(&mut self) -> bool {
        self.ask("agent_owns_typing")
    }
    fn pty_takes_input(&mut self) -> bool {
        self.ask("pty_takes_input")
    }
    fn input_box_visible(&mut self) -> bool {
        self.ask("input_box_visible")
    }
}

#[test]
fn pane_key_typed_route_scenarios() {
    let scenarios: &[(&str, bool, &[&'static str])] = &[
        ("plain key, pty takes it", false, &["pty_takes_input"]),
        ("plain key, nothing takes the pty", false, &[]),
        ("ssh choice card up", false, &["ssh_choice_card_up"]),
        (
            "ssh setup holds a plain key",
            false,
            &["ssh_setup_holds_key"],
        ),
        (
            "interrupt held by the ssh connect",
            true,
            &["ssh_interrupt_held"],
        ),
        (
            "interrupt passes the ssh connect",
            true,
            &["pty_takes_input"],
        ),
        (
            "interrupt: the setup hold is not asked",
            true,
            &["ssh_setup_holds_key", "pty_takes_input"],
        ),
        (
            "enter queue takes the key",
            false,
            &["enter_queue_takes_key"],
        ),
        ("powershell setup holds", false, &["powershell_setup_holds"]),
        (
            "approval card wins over the pty",
            false,
            &["on_approval_card", "pty_takes_input"],
        ),
        (
            "settled ai target",
            false,
            &["on_settled_ai_target", "pty_takes_input"],
        ),
        (
            "agent owns typing",
            false,
            &["agent_owns_typing", "pty_takes_input"],
        ),
    ];

    let mut out = Vec::new();
    for (name, interrupt_key, yes) in scenarios {
        let mut probes = Scripted::answering(yes);
        let route = route_typed(*interrupt_key, &mut probes);
        out.push(format!(
            "{name}\n  interrupt_key={interrupt_key} yes={yes:?}\n  asked: {}\n  route: {route:?}",
            probes.asked.join(", ")
        ));
    }
    snap!("pane_key_typed_route_scenarios", out.join("\n\n"));
}

#[test]
fn pane_key_after_pty_attempt_table() {
    let mut rows = Vec::new();
    for bits in 0..16u8 {
        let tail = TypedTail {
            approval_card: bits & 1 != 0,
            settled_ai_target: false,
            agent_owns_typing: bits & 2 != 0,
            try_pty: true,
        };
        for written in [false, true] {
            for (pty_takes, input_visible) in [(false, false), (true, false), (true, true)] {
                let mut yes = Vec::new();
                if pty_takes {
                    yes.push("pty_takes_input");
                }
                if input_visible {
                    yes.push("input_box_visible");
                }
                let mut probes = Scripted::answering(&yes);
                let action = after_pty_attempt(&tail, written, &mut probes);
                rows.push(format!(
                    "approval_card={} agent_owns_typing={} written={written} pty_takes={pty_takes} input_visible={input_visible} -> {action:?}",
                    tail.approval_card, tail.agent_owns_typing
                ));
            }
        }
    }
    snap!("pane_key_after_pty_attempt_table", rows.join("\n"));
}
