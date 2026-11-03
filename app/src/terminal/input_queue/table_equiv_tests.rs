//! Table-vs-legacy equivalence (#1529, part of #1424).
//!
//! The legacy destination decision that can be called as a pure function is
//! [`destination_for`] (`destination.rs`: `DestFacts` + `EntryKind` -> `Destination`, what
//! `InputQueue::route` evaluates at drain time). The view's inline chain that `shadow.rs` compares
//! at runtime is not callable without a view, so this is the equivalence the unit tests CAN state.
//! [`decide`] is asked the same question over the same facts and the two must agree, except for
//! the differences listed in [`ALLOWED`]. Every allowed difference names the row/decision it comes
//! from and its issue; a difference that matches no entry fails the test, and an entry that
//! matches no difference (a stale one) fails it too, so the list can only shrink or be argued for.
//!
//! The fact space is every combination of `DestFacts`' five booleans and `drop_reason` (None or
//! the ssh dialog) for each entry kind legacy treats distinctly: typed text, a terminal paste,
//! an editor paste, passive raw bytes and the four signals. `DestFacts` is mapped to
//! `RouteFacts` as follows (everything else is an idle shell with nothing queued, no gate, ssh
//! idle, a running command):
//!
//! * `pty_takes_input` -> `active_block_started` + `session_pending_or_bootstrapped` +
//!   `is_bootstrapped` (so `pty_typed(f)`), `approval_card` -> the flag and the stamped focus,
//!   `agent_owns_pty`, `input_box_visible`, `editor_focused` -> the same fields,
//!   `drop_reason = Some(SshDialogOrPassword)` -> `ssh_dialog_up`;
//! * `drop_reason` values other than the ssh dialog are computed by the host from facts the table
//!   reads directly (`AgentControl`, `RefusedHiddenInput`), so they are not an input here;
//! * `pty_open` only matters for a line-run pin, which this file does not use (pin: `None`).
//!
//! Not compared, and why: `Enter`, `Control` and `Seq` (legacy `destination_for` treats them
//! exactly like text, asserted below, while the real legacy decision for them is the view's
//! keydown predicate `pty_control`/`pty_seq`, which `destination_for` never modelled);
//! `PtyBytes`, `Held` and `EditorKey` (legacy answers by kind alone and there is no single table
//! class for them); the line-run pin (`apply_pin` has its own tests in `table_tests.rs`).
//! Built by assignment, so a field added to `RouteFacts` or `DestFacts` does not break this file.

use crate::terminal::input_queue::table::{Class, DropWhy, Intent, Outcome, decide};
use crate::terminal::input_queue::{
    DestFacts, Destination, DropReason, EntryKind, FocusSnap, Origin, RouteFacts, SignalKind,
    Stamp, destination_for,
};

/// The table's answer in the legacy vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Td {
    Pty,
    Editor,
    Agent,
    Drop(DropWhy),
    /// The ssh setup dialog swallowed it.
    Swallow,
    /// A hold, a hint, a queue: nothing in this fact space may produce one.
    Other,
}

fn td_of(outcome: &Outcome) -> Td {
    match outcome {
        Outcome::Pty(_)
        | Outcome::PasteToPty { .. }
        | Outcome::SignalToPty(_)
        | Outcome::SigintForeground
        | Outcome::AbortBytes
        | Outcome::PowershellAbort
        | Outcome::SignalAtGates => Td::Pty,
        Outcome::EditorInsert | Outcome::EditorAction | Outcome::EditorSubmit => Td::Editor,
        Outcome::AgentInput => Td::Agent,
        Outcome::Drop(why) => Td::Drop(*why),
        Outcome::Swallow(_) => Td::Swallow,
        Outcome::Hold(_)
        | Outcome::HoldSignal
        | Outcome::Queue
        | Outcome::Stash
        | Outcome::Hint => Td::Other,
    }
}

/// Whether legacy's answer and the table's are the same decision.
fn same(legacy: Destination, table: Td) -> bool {
    match (legacy, table) {
        (Destination::Pty, Td::Pty)
        | (Destination::Editor, Td::Editor)
        | (Destination::AgentInput, Td::Agent)
        | (Destination::Drop(DropReason::SshDialogOrPassword), Td::Swallow) => true,
        (Destination::Drop(DropReason::AgentControl), Td::Drop(DropWhy::AgentControl)) => true,
        (
            Destination::Drop(DropReason::RefusedHiddenInput),
            Td::Drop(DropWhy::RefusedHiddenInput),
        ) => true,
        _ => false,
    }
}

/// An editor-owned paste id (`pane::EDITOR_PASTE_BIT`).
const EDITOR_PASTE: u64 = 1 << 63;
/// A terminal-owned paste id.
const TERMINAL_PASTE: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Text,
    TerminalPaste,
    EditorPaste,
    Raw,
    Signal(SignalKind),
}

const KINDS: [Kind; 8] = [
    Kind::Text,
    Kind::TerminalPaste,
    Kind::EditorPaste,
    Kind::Raw,
    Kind::Signal(SignalKind::Interrupt),
    Kind::Signal(SignalKind::Eof),
    Kind::Signal(SignalKind::Suspend),
    Kind::Signal(SignalKind::Quit),
];

fn entry_of(kind: Kind) -> EntryKind {
    match kind {
        Kind::Text => EntryKind::Text("a".to_owned()),
        Kind::TerminalPaste => EntryKind::Paste {
            paste_id: TERMINAL_PASTE,
            payload: Some(b"p".to_vec()),
        },
        Kind::EditorPaste => EntryKind::Paste {
            paste_id: EDITOR_PASTE,
            payload: Some(b"p".to_vec()),
        },
        Kind::Raw => EntryKind::Raw(vec![0x1b]),
        Kind::Signal(signal) => EntryKind::Signal(signal),
    }
}

/// One point of the compared space.
#[derive(Clone, Copy, Debug)]
struct Case {
    kind: Kind,
    ssh_dialog: bool,
    approval_card: bool,
    agent_owns_pty: bool,
    pty_takes_input: bool,
    input_box_visible: bool,
    editor_focused: bool,
}

impl Case {
    fn dest_facts(&self) -> DestFacts {
        DestFacts {
            drop_reason: self.ssh_dialog.then_some(DropReason::SshDialogOrPassword),
            approval_card: self.approval_card,
            agent_owns_pty: self.agent_owns_pty,
            pty_takes_input: self.pty_takes_input,
            input_box_visible: self.input_box_visible,
            editor_focused: self.editor_focused,
            pty_open: true,
        }
    }

    fn route_facts(&self) -> RouteFacts {
        let mut f = RouteFacts::default();
        f.model_consistent = true;
        f.handoff_known = true;
        // A running command: the signals' keydown predicate needs one (legacy sends a signal to
        // the command unconditionally).
        f.long_running = true;
        f.active_block_started = self.pty_takes_input;
        f.session_pending_or_bootstrapped = self.pty_takes_input;
        f.is_bootstrapped = self.pty_takes_input;
        f.approval_card = self.approval_card;
        f.agent_owns_pty = self.agent_owns_pty;
        f.input_box_visible = self.input_box_visible;
        f.editor_focused = self.editor_focused;
        f.ssh_dialog_up = self.ssh_dialog;
        f
    }

    fn intent(&self) -> Intent {
        let (class, origin) = match self.kind {
            Kind::Text => (Class::Typed, Origin::Keyboard),
            Kind::TerminalPaste => (Class::Paste, Origin::Keyboard),
            Kind::EditorPaste => (Class::Paste, Origin::Editor),
            Kind::Raw => (Class::Raw, Origin::Keyboard),
            Kind::Signal(signal) => (Class::Signal(signal), Origin::Keyboard),
        };
        let focus = if self.approval_card {
            FocusSnap::ApprovalCard
        } else {
            FocusSnap::Other
        };
        // `delivering`: the entry has its place in the queue; nothing is queued ahead of it.
        Intent::new(class, origin)
            .stamped(Stamp {
                focus,
                ..Stamp::UNSTAMPED
            })
            .delivering()
    }
}

fn all_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for kind in KINDS {
        for bits in 0..64u32 {
            let bit = |n: u32| bits & (1 << n) != 0;
            cases.push(Case {
                kind,
                ssh_dialog: bit(0),
                approval_card: bit(1),
                agent_owns_pty: bit(2),
                pty_takes_input: bit(3),
                input_box_visible: bit(4),
                editor_focused: bit(5),
            });
        }
    }
    cases
}

/// One documented, intended difference: where it comes from, its issue, and exactly which
/// (kind, facts, legacy answer, table answer) it covers.
struct Allowed {
    id: &'static str,
    issue: &'static str,
    why: &'static str,
    covers: fn(&Case, Destination, Td) -> bool,
}

fn is_paste(case: &Case) -> bool {
    case.kind == Kind::TerminalPaste
}

/// Every intended difference between `destination_for` and `decide` over the compared space. A
/// new line here needs an issue and a reason; do not widen a `covers` to make a failure go away.
const ALLOWED: &[Allowed] = &[
    Allowed {
        id: "B12 hidden-input text",
        issue: "#1424 (B12, OD-4)",
        why: "legacy destination_for drops text the pty does not take when the input box is \
              hidden; the real legacy path (after_pty_attempt) inserts into the editor and drops \
              only when the pty REFUSES the write (D8). Fixed in destination_for_fixed.",
        covers: |c, l, t| {
            c.kind == Kind::Text
                && matches!(l, Destination::Drop(DropReason::RefusedHiddenInput))
                && t == Td::Editor
                && !c.pty_takes_input
                && !c.input_box_visible
        },
    },
    Allowed {
        id: "D6 focused editor, pty takes text",
        issue: "#1424 (D6, pin)",
        why: "legacy sends text to the editor whenever the editor is focused; warpui routes keys \
              to a focused editor before they reach the sink, so a typed key that reaches decide \
              goes by the pty predicate (D6). A line that began in the editor is kept there by \
              the line-run pin (apply_pin), not by this row.",
        covers: |c, l, t| {
            c.kind == Kind::Text
                && l == Destination::Editor
                && t == Td::Pty
                && c.editor_focused
                && c.pty_takes_input
        },
    },
    Allowed {
        id: "paste: hidden box, pty not taking",
        issue: "#1424 (paste_routing_via_table, D6)",
        why: "legacy destination_for drops; a terminal paste is routed by paste_routing, which \
              the table follows: not the editor, so it goes to the pty (the refusal is the \
              write's).",
        covers: |c, l, t| {
            is_paste(c)
                && matches!(l, Destination::Drop(DropReason::RefusedHiddenInput))
                && t == Td::Pty
                && !c.editor_focused
                && !c.pty_takes_input
                && !c.input_box_visible
        },
    },
    Allowed {
        id: "paste: visible box, nothing focused or bootstrapped",
        issue: "#1424 (paste_routing_via_table, D6)",
        why: "paste_routing uses the input box only when it is focused or the shell is \
              bootstrapped; destination_for inserted whenever the pty did not take input.",
        covers: |c, l, t| {
            is_paste(c)
                && l == Destination::Editor
                && t == Td::Pty
                && !c.editor_focused
                && !c.pty_takes_input
                && c.input_box_visible
        },
    },
    Allowed {
        id: "paste: focused editor, box hidden",
        issue: "#1424 (paste_routing_via_table, D6)",
        why: "paste_routing needs the input box to be shown for the editor to take a paste; \
              destination_for sent it to a focused editor regardless.",
        covers: |c, l, t| {
            is_paste(c)
                && l == Destination::Editor
                && t == Td::Pty
                && c.editor_focused
                && !c.input_box_visible
        },
    },
    Allowed {
        id: "paste: bootstrapped shell, visible box",
        issue: "#1424 (paste_routing_via_table, D7)",
        why: "paste_routing puts a paste into a shown input box once the shell is bootstrapped, \
              even though the pty would take text; destination_for sent it to the pty.",
        covers: |c, l, t| {
            is_paste(c)
                && l == Destination::Pty
                && t == Td::Editor
                && !c.editor_focused
                && c.pty_takes_input
                && c.input_box_visible
        },
    },
    Allowed {
        id: "editor paste in the ssh dialog",
        issue: "#1113, #1424 (H2)",
        why: "legacy routes an editor-owned paste to the editor before looking at drop_reason; \
              the table swallows every paste while the ssh setup dialog owns the keyboard (H2 \
              judges the class, not the origin).",
        covers: |c, l, t| {
            c.kind == Kind::EditorPaste
                && l == Destination::Editor
                && t == Td::Swallow
                && c.ssh_dialog
        },
    },
    Allowed {
        id: "raw bytes in the ssh dialog",
        issue: "#1113, #1424 (H2, D12)",
        why: "legacy drops passive pty bytes while a dialog or password prompt is up; H2 judges \
              typed text, Enter, paste and control keys only, and passive bytes (mouse reports) \
              are not typing.",
        covers: |c, l, t| {
            c.kind == Kind::Raw
                && matches!(l, Destination::Drop(DropReason::SshDialogOrPassword))
                && t == Td::Pty
        },
    },
    Allowed {
        id: "raw bytes while the agent OWNS the pty",
        issue: "#1424 (OD-3, D3)",
        why: "legacy dropped passive bytes whenever agent_owns_pty; the table drops them only \
              while the agent is IN CONTROL (agent_in_control), as the view always did.",
        covers: |c, l, t| {
            c.kind == Kind::Raw
                && matches!(l, Destination::Drop(DropReason::AgentControl))
                && t == Td::Pty
                && c.agent_owns_pty
        },
    },
    Allowed {
        id: "raw bytes, pty not taking input",
        issue: "#1424 (D12)",
        why: "legacy destination_for drops raw bytes the pty does not take with the box hidden; \
              the table decides the pty and leaves the refusal to the write (raw bytes have no \
              editor fallback).",
        covers: |c, l, t| {
            c.kind == Kind::Raw
                && matches!(l, Destination::Drop(DropReason::RefusedHiddenInput))
                && t == Td::Pty
                && !c.pty_takes_input
        },
    },
    Allowed {
        id: "^Z / ^\\ while the agent owns the pty",
        issue: "#1424 (OD-1, D10)",
        why: "legacy sends every signal to the command; only ^C and ^D are abort keys that an \
              agent-owned pty does not block (pty_control), ^Z and ^\\ meet D10 and are dropped \
              with a log, as the keydown path always did.",
        covers: |c, l, t| {
            matches!(c.kind, Kind::Signal(SignalKind::Suspend | SignalKind::Quit))
                && l == Destination::Pty
                && t == Td::Drop(DropWhy::NotLongRunning)
                && c.agent_owns_pty
        },
    },
];

/// What the comparison made of one case.
pub(in crate::terminal::input_queue) enum Verdict {
    Same,
    /// Differs, covered by these entries of [`ALLOWED`].
    Allowed(Vec<usize>),
    /// Differs and no entry covers it.
    Unlisted(String),
}

fn compare(case: &Case) -> Verdict {
    let legacy = destination_for(&entry_of(case.kind), &case.dest_facts());
    let plan = decide(&case.intent(), &case.route_facts(), None);
    let table = td_of(&plan.outcome);
    assert_ne!(
        table,
        Td::Other,
        "a hold or hint in a fact space with no gate, no queue and ssh idle: {case:?} -> {plan:?}"
    );
    if same(legacy, table) {
        return Verdict::Same;
    }
    let covering: Vec<usize> = ALLOWED
        .iter()
        .enumerate()
        .filter(|(_, allowed)| (allowed.covers)(case, legacy, table))
        .map(|(index, _)| index)
        .collect();
    if covering.is_empty() {
        Verdict::Unlisted(format!(
            "{case:?}: legacy {legacy:?}, table {table:?} (row {})",
            plan.row.name()
        ))
    } else {
        Verdict::Allowed(covering)
    }
}

/// The property model's `Decide` op (#1529): the same comparison on the model's CURRENT facts.
/// Only meaningful with no open gate and nothing queued (the compared space); the caller checks
/// that. `pick` selects the entry kind; a closed pty is outside the space and is skipped.
pub(in crate::terminal::input_queue) fn check_live_facts(
    facts: &DestFacts,
    pick: u8,
) -> Result<(), String> {
    if !facts.pty_open {
        return Ok(());
    }
    let case = Case {
        kind: KINDS[usize::from(pick) % KINDS.len()],
        ssh_dialog: facts.drop_reason == Some(DropReason::SshDialogOrPassword),
        approval_card: facts.approval_card,
        agent_owns_pty: facts.agent_owns_pty,
        pty_takes_input: facts.pty_takes_input,
        input_box_visible: facts.input_box_visible,
        editor_focused: facts.editor_focused,
    };
    match compare(&case) {
        Verdict::Unlisted(message) => Err(message),
        _ => Ok(()),
    }
}

/// Walks the space. Returns the differences no entry of [`ALLOWED`] covers, and how many
/// differences each entry covered.
fn walk() -> (Vec<String>, Vec<usize>) {
    let mut unlisted = Vec::new();
    let mut hits = vec![0usize; ALLOWED.len()];
    for case in all_cases() {
        match compare(&case) {
            Verdict::Same => {}
            Verdict::Allowed(covering) => {
                for index in covering {
                    hits[index] += 1;
                }
            }
            Verdict::Unlisted(message) => unlisted.push(message),
        }
    }
    (unlisted, hits)
}

#[test]
fn decide_agrees_with_destination_for_except_for_the_listed_differences() {
    let (unlisted, _) = walk();
    assert!(
        unlisted.is_empty(),
        "{} difference(s) between destination_for and table::decide are not in ALLOWED \
         (each needs an issue and a reason, or a fix):\n{}",
        unlisted.len(),
        unlisted.join("\n")
    );
}

#[test]
fn every_allowed_difference_still_occurs() {
    let (_, hits) = walk();
    let stale: Vec<String> = ALLOWED
        .iter()
        .zip(&hits)
        .filter(|(_, count)| **count == 0)
        .map(|(allowed, _)| format!("{} ({})", allowed.id, allowed.issue))
        .collect();
    assert!(
        stale.is_empty(),
        "ALLOWED entries that no case reaches any more (remove them): {stale:?}"
    );
}

#[test]
fn every_allowed_difference_names_an_issue_and_a_reason() {
    for allowed in ALLOWED {
        assert!(
            allowed.issue.contains('#') || allowed.issue.contains("OD-"),
            "{}: no issue",
            allowed.id
        );
        assert!(allowed.why.len() > 40, "{}: no reason", allowed.id);
    }
}

#[test]
fn the_compared_space_is_what_the_docs_say() {
    assert!(crate::terminal::input_queue::pane::is_editor_paste_id(
        EDITOR_PASTE
    ));
    assert!(!crate::terminal::input_queue::pane::is_editor_paste_id(
        TERMINAL_PASTE
    ));
    // Enter, Control and Seq are not compared because legacy gives them text's answer: if that
    // stops being true they need their own comparison.
    for case in all_cases().iter().filter(|c| c.kind == Kind::Text) {
        let facts = case.dest_facts();
        let text = destination_for(&entry_of(Kind::Text), &facts);
        for kind in [
            EntryKind::Enter,
            EntryKind::Control("\u{8}"),
            EntryKind::Seq("\u{1b}[A"),
        ] {
            assert_eq!(
                destination_for(&kind, &facts),
                text,
                "{kind:?} no longer routes like text under {facts:?}: compare it with decide"
            );
        }
    }
    // Legacy answers by kind alone for these.
    let facts = DestFacts::default();
    assert_eq!(
        destination_for(
            &EntryKind::PtyBytes {
                bytes: vec![1],
                takes_over_agent_control: true
            },
            &facts
        ),
        Destination::Pty
    );
    assert_eq!(
        destination_for(
            &EntryKind::Held {
                class: 0,
                text: String::new()
            },
            &facts
        ),
        Destination::Pty
    );
}
