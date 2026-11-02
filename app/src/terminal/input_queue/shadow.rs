//! The shadow comparison of the destination table (#1424, step 3): where legacy sent an input
//! against where the table would have.
//!
//! In `PHOSPHOR_QUEUE_DESTINATION=shadow` the view decides every input with the legacy chain
//! exactly as before, and next to it asks [`super::table::decide`] with the facts and the stamp of
//! the same moment. At the legacy LEAF (the last action that puts the input somewhere) the view
//! calls [`note_leaf`] with the [`DestClass`] legacy chose; the wrapper around the legacy entry
//! point then [`compare`]s it with the table's plan and [`report`]s a disagreement as
//! `kh_dest_disagree row=.. legacy=.. table=.. flags=..` (`input_order` target, debug). Shadow
//! proves DESTINATIONS only; ordering is proven by table-mode tests. It changes no behaviour in any
//! mode except that log.
//!
//! Nothing here logs typed text: a line carries the row, the two destination classes and a few
//! boolean/enum facts. Known differences are tagged `expected=..` so the log stays quiet about
//! them: `OD1` (^Z / ^\\ are signals now), `OD5` (a signal in the ssh setup window is held, not
//! dropped), `OD7` (a key stamped before the ssh prompt is dropped), `B3` (the table logs a drop
//! legacy did silently), `B7` (the published agent flag was one publication old), `B13` (a control
//! key reaches the pty while the agent owns it, through the hand-off).
//!
//! The comparison is pure; the only state is two thread-locals (the leaf the legacy chain noted,
//! and, in tests, the recorded lines).

use std::cell::Cell;

use super::facts::RouteFacts;
use super::table::{Class, DropWhy, Intent, Outcome, Plan, Row, pty_typed};
use super::{DestinationMode, SignalKind, destination_mode};

/// Where an input ended up, coarse enough for legacy and the table to be compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestClass {
    /// Written to the pty (or handed to a hold of the hand-off, which writes it later).
    Pty,
    /// The input editor (the agent's steer text is the editor too).
    Editor,
    /// Held behind a gate or in the ssh hold, to be delivered later.
    Held,
    /// Swallowed by the ssh setup dialog.
    Swallowed,
    /// Dropped.
    Dropped,
    /// The ssh choice card showed its hint.
    Hint,
}

impl DestClass {
    pub fn name(self) -> &'static str {
        match self {
            DestClass::Pty => "pty",
            DestClass::Editor => "editor",
            DestClass::Held => "held",
            DestClass::Swallowed => "swallowed",
            DestClass::Dropped => "dropped",
            DestClass::Hint => "hint",
        }
    }
}

/// The class of the table's outcome.
pub fn class_of(outcome: &Outcome) -> DestClass {
    match outcome {
        Outcome::Pty(_)
        | Outcome::PasteToPty { .. }
        | Outcome::SignalToPty(_)
        | Outcome::SigintForeground
        | Outcome::AbortBytes
        | Outcome::PowershellAbort
        | Outcome::SignalAtGates => DestClass::Pty,
        Outcome::EditorInsert
        | Outcome::EditorAction
        | Outcome::EditorSubmit
        | Outcome::AgentInput => DestClass::Editor,
        Outcome::Hold(_) | Outcome::HoldSignal | Outcome::Queue | Outcome::Stash => DestClass::Held,
        Outcome::Swallow(_) => DestClass::Swallowed,
        Outcome::Drop(_) => DestClass::Dropped,
        Outcome::Hint => DestClass::Hint,
    }
}

/// One difference between legacy and the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disagreement {
    pub row: Row,
    pub legacy: DestClass,
    pub table: DestClass,
    pub flags: String,
    /// A known cause (`OD1`, `OD5`, `OD7`, `B3`, `B7`, `B13`): expected, not a regression.
    pub expected: Option<&'static str>,
}

impl Disagreement {
    /// The `input_order` detail: `row=.. legacy=.. table=.. flags=..[ expected=..]`.
    pub fn detail(&self) -> String {
        let mut line = format!(
            "row={} legacy={} table={} flags={}",
            self.row.name(),
            self.legacy.name(),
            self.table.name(),
            self.flags
        );
        if let Some(tag) = self.expected {
            line.push_str(" expected=");
            line.push_str(tag);
        }
        line
    }
}

/// The known cause of a difference on `plan`, if there is one.
pub fn expected_tag(intent: &Intent, plan: &Plan, f: &RouteFacts) -> Option<&'static str> {
    if matches!(
        intent.class,
        Class::Signal(SignalKind::Suspend | SignalKind::Quit)
    ) {
        return Some("OD1");
    }
    if matches!(plan.row, Row::P1a | Row::P1f) {
        return Some("OD5");
    }
    if plan.row == Row::H4 {
        return Some("OD7");
    }
    if matches!(plan.outcome, Outcome::Drop(DropWhy::RefusedHiddenInput)) {
        return Some("B3");
    }
    if matches!(intent.class, Class::Control | Class::Enter)
        && f.agent_owns_pty
        && f.handoff_routes
        && matches!(plan.outcome, Outcome::Pty(_))
    {
        return Some("B13");
    }
    if f.agent_in_control && !f.agent_in_control_exact {
        return Some("B7");
    }
    None
}

/// A short, text-free description of the facts that decided: for the log line.
pub fn flags(f: &RouteFacts) -> String {
    format!(
        "ssh={} long={} started={} agent_owns={} agent_ctl={} handoff={} box={} pty_typed={} gates={:#x} model_ok={}",
        match f.ssh {
            super::SshPhase::Idle => "idle",
            super::SshPhase::Holding => "holding",
            super::SshPhase::PromptUp { .. } => "prompt",
        },
        u8::from(f.long_running),
        u8::from(f.active_block_started),
        u8::from(f.agent_owns_pty),
        u8::from(f.agent_in_control),
        u8::from(f.handoff_routes),
        u8::from(f.input_box_visible),
        u8::from(pty_typed(f)),
        f.gates.0,
        u8::from(f.model_consistent),
    )
}

/// Compares what legacy did (`legacy`) with the table's `plan`. `None` when they agree.
pub fn compare(
    intent: &Intent,
    plan: &Plan,
    f: &RouteFacts,
    legacy: DestClass,
) -> Option<Disagreement> {
    let table = class_of(&plan.outcome);
    if table == legacy {
        return None;
    }
    Some(Disagreement {
        row: plan.row,
        legacy,
        table,
        flags: flags(f),
        expected: expected_tag(intent, plan, f),
    })
}

/// [`compare`], and [`report`] a disagreement. Returns it (for tests).
pub fn check(
    intent: &Intent,
    plan: &Plan,
    f: &RouteFacts,
    legacy: DestClass,
) -> Option<Disagreement> {
    let disagreement = compare(intent, plan, f, legacy)?;
    report(&disagreement);
    Some(disagreement)
}

/// Logs a disagreement (`kh_dest_disagree`, `input_order` target, debug).
pub fn report(disagreement: &Disagreement) {
    #[cfg(test)]
    RECORDED.with(|lines| lines.borrow_mut().push(disagreement.detail()));
    warpui::input_order::log_key_hop("kh_dest_disagree", || disagreement.detail());
}

/// Whether the shadow comparison should run now: the mode is `shadow` and someone listens
/// (`RUST_LOG=warp::input_order=debug`), so a process nobody is watching pays nothing. A test
/// that forces `shadow` ([`super::facts::set_destination_mode_for_test`]) always runs it.
pub fn wanted() -> bool {
    #[cfg(test)]
    if let Some(mode) = super::facts::mode_forced_for_test() {
        return mode == DestinationMode::Shadow;
    }
    destination_mode() == DestinationMode::Shadow && warpui::input_order::enabled()
}

thread_local! {
    /// The destination class the legacy chain reached its leaf with, noted by the leaf.
    static LEAF: Cell<Option<DestClass>> = const { Cell::new(None) };
}

/// Starts a legacy entry point: forgets the leaf of the previous one.
pub fn begin_leaf() {
    LEAF.with(|leaf| leaf.set(None));
}

/// The legacy chain reached a leaf with `class`. The first leaf of an entry point wins (a nested
/// write inside it must not overwrite the decision).
pub fn note_leaf(class: DestClass) {
    LEAF.with(|leaf| {
        if leaf.get().is_none() {
            leaf.set(Some(class));
        }
    });
}

/// The leaf the entry point noted, once.
pub fn take_leaf() -> Option<DestClass> {
    LEAF.with(Cell::take)
}

#[cfg(test)]
thread_local! {
    static RECORDED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The disagreement lines reported on this thread since the last call (tests).
#[cfg(test)]
pub fn take_recorded() -> Vec<String> {
    RECORDED.with(|lines| std::mem::take(&mut *lines.borrow_mut()))
}

#[cfg(test)]
#[path = "shadow_tests.rs"]
mod tests;
