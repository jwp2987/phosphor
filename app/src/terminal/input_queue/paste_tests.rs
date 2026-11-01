//! Phase 1 (#1353): the paste ordering the terminal view and the editor now run on this queue,
//! exercised through a host that records what is delivered, with the same entry shapes the
//! wiring uses (`PtyBytes` keys, a gated `Paste` entry, `Signal` for ^C / ^D).

use std::time::Duration;

use super::*;

#[derive(Default)]
struct Recorder {
    events: Vec<String>,
    armed: Vec<(GateToken, GateKind, Duration)>,
    warns: usize,
}

impl Host for Recorder {
    fn facts(&self) -> DestFacts {
        DestFacts {
            pty_takes_input: true,
            pty_open: true,
            ..DestFacts::default()
        }
    }

    fn deliver(
        &mut self,
        entry: &InputEntry,
        _dest: Destination,
        _follow: &mut FollowUps,
    ) -> DeliverOutcome {
        self.events.push(match &entry.kind {
            EntryKind::PtyBytes { bytes, .. } => String::from_utf8_lossy(bytes).into_owned(),
            EntryKind::Paste { paste_id, .. } => format!("PASTE{paste_id}"),
            EntryKind::Signal(SignalKind::Interrupt) => "^C".to_owned(),
            EntryKind::Signal(SignalKind::Eof) => "^D".to_owned(),
            other => format!("{other:?}"),
        });
        DeliverOutcome::Delivered
    }

    fn discarded(&mut self, entry: &InputEntry, why: DiscardReason) {
        let what = match &entry.kind {
            EntryKind::Paste { paste_id, .. } => format!("paste{paste_id}"),
            other => format!("{other:?}"),
        };
        self.events.push(format!("discard:{what}:{why:?}"));
    }

    fn arm_give_up(&mut self, token: GateToken, kind: GateKind, after: Duration) {
        self.armed.push((token, kind, after));
    }

    fn warn(&mut self, _line: &str) {
        self.warns += 1;
    }
}

fn key(s: &str) -> EntryKind {
    EntryKind::PtyBytes {
        bytes: s.as_bytes().to_vec(),
        takes_over_agent_control: true,
    }
}

fn paste(id: u64) -> EntryKind {
    EntryKind::Paste {
        paste_id: id,
        payload: None,
    }
}

fn start_paste(q: &mut InputQueue, h: &mut Recorder, id: u64) -> GateToken {
    q.offer_gated(h, paste(id), GateSpec::new(GateKind::PasteReading, id))
        .1
}

fn read_ok(q: &mut InputQueue, h: &mut Recorder, id: u64) {
    q.on_paste_read(h, id, Ok(Vec::new()));
}

/// Paste and typed keys + Enter in EVERY interleaving: the paste starts after `p` keys, its read
/// finishes after `r >= p` keys; delivery is always typed order with the paste where it started.
#[test]
fn paste_and_typed_keys_with_enter_deliver_in_typed_order_in_every_interleaving() {
    let typed = ["l", "s", "\r"];
    for p in 0..=typed.len() {
        for r in p..=typed.len() {
            let mut q = InputQueue::new();
            let mut h = Recorder::default();
            for i in 0..=typed.len() {
                if i == p {
                    start_paste(&mut q, &mut h, 1);
                }
                if i == r {
                    read_ok(&mut q, &mut h, 1);
                }
                if i < typed.len() {
                    q.offer(&mut h, key(typed[i]));
                }
            }
            let mut expected: Vec<String> = typed[..p].iter().map(|s| s.to_string()).collect();
            expected.push("PASTE1".to_owned());
            expected.extend(typed[p..].iter().map(|s| s.to_string()));
            assert_eq!(h.events, expected, "paste at {p}, read done at {r}");
            assert!(q.is_idle(), "nothing left held (paste at {p}, read at {r})");
        }
    }
}

#[test]
fn a_slow_read_holds_every_key_behind_it_until_the_result_event() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    start_paste(&mut q, &mut h, 7);
    for k in ["l", "s", "\r"] {
        q.offer(&mut h, key(k));
    }
    assert!(h.events.is_empty(), "nothing leaves before the read result");
    assert_eq!(q.queued_len(), 4);
    read_ok(&mut q, &mut h, 7);
    assert_eq!(h.events, ["PASTE7", "l", "s", "\r"]);
}

#[test]
fn a_failed_read_discards_only_the_paste_and_releases_the_keys_in_order() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    start_paste(&mut q, &mut h, 3);
    q.offer(&mut h, key("a"));
    q.offer(&mut h, key("\r"));
    q.on_paste_read(&mut h, 3, Err(()));
    assert_eq!(
        h.events,
        ["discard:paste3:PasteFailed", "a", "\r"],
        "the failure is reported, the keys follow in order"
    );
    assert!(q.is_idle());
}

#[test]
fn backstop_gives_up_once_and_a_late_result_is_ignored() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    let token = q
        .offer_gated(
            &mut h,
            paste(5),
            GateSpec::new(GateKind::PasteReading, 5).with_cap(Some(Duration::from_millis(100))),
        )
        .1;
    assert_eq!(
        h.armed,
        [(token, GateKind::PasteReading, Duration::from_millis(100))],
        "exactly one timer, with the requested cap"
    );
    q.offer(&mut h, key("l"));
    q.offer(&mut h, key("s"));
    q.offer(&mut h, key("\r"));
    assert!(h.events.is_empty());

    assert!(q.give_up(&mut h, token), "the cap fires for the open gate");
    assert_eq!(h.warns, 1, "a give-up is always logged");
    assert_eq!(
        h.events,
        ["discard:paste5:PasteNotReady", "l", "s", "\r"],
        "keys released in typing order, the paste never written"
    );

    // The reader answers after all: nothing is delivered, nothing is held again.
    let before = h.events.clone();
    read_ok(&mut q, &mut h, 5);
    assert_eq!(h.events, before, "the late result is ignored");
    assert!(!q.give_up(&mut h, token), "the cap is one-shot");
    assert_eq!(h.warns, 1);
    assert!(q.is_idle());
}

#[test]
fn a_result_before_the_cap_makes_the_cap_a_no_op() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    let token = start_paste(&mut q, &mut h, 1);
    read_ok(&mut q, &mut h, 1);
    assert_eq!(h.events, ["PASTE1"]);
    assert!(!q.give_up(&mut h, token));
    assert_eq!(h.warns, 0, "no warning for a gate the event already closed");
}

#[test]
fn no_cap_means_no_timer() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    q.offer_gated(
        &mut h,
        paste(1),
        GateSpec::new(GateKind::PasteReading, 1).with_cap(None),
    );
    assert!(h.armed.is_empty());
}

#[test]
fn ctrl_c_during_a_pending_paste_cancels_it_delivers_earlier_keys_then_the_abort() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    q.offer(&mut h, key("a"));
    start_paste(&mut q, &mut h, 2);
    q.offer(&mut h, key("b"));
    q.offer(&mut h, key("c"));
    assert_eq!(h.events, ["a"]);
    q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(
        h.events,
        ["a", "discard:paste2:PasteCancelled", "b", "c", "^C"],
        "typed-before keys first, the paste dropped, the abort last"
    );
    assert!(q.is_idle());
    // The reader answers later: ignored.
    let before = h.events.clone();
    read_ok(&mut q, &mut h, 2);
    assert_eq!(h.events, before);
}

#[test]
fn ctrl_d_cancels_a_pending_paste_too() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    start_paste(&mut q, &mut h, 2);
    q.offer(&mut h, key("x"));
    q.offer(&mut h, EntryKind::Signal(SignalKind::Eof));
    assert_eq!(h.events, ["discard:paste2:PasteCancelled", "x", "^D"]);
}

#[test]
fn a_paste_behind_a_paste_waits_for_it_and_neither_is_lost() {
    // Out-of-order results: the second paste's read finishes first and must still wait.
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    start_paste(&mut q, &mut h, 1);
    q.offer(&mut h, key("a"));
    start_paste(&mut q, &mut h, 2);
    q.offer(&mut h, key("b"));
    read_ok(&mut q, &mut h, 2);
    assert!(h.events.is_empty(), "paste 2 is ready but paste 1 is first");
    read_ok(&mut q, &mut h, 1);
    assert_eq!(h.events, ["PASTE1", "a", "PASTE2", "b"]);
}

#[test]
fn a_failed_first_paste_does_not_swallow_the_second() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    start_paste(&mut q, &mut h, 1);
    start_paste(&mut q, &mut h, 2);
    q.offer(&mut h, key("z"));
    q.on_paste_read(&mut h, 1, Err(()));
    read_ok(&mut q, &mut h, 2);
    assert_eq!(h.events, ["discard:paste1:PasteFailed", "PASTE2", "z"]);
}

#[test]
fn idle_queue_takes_no_gate_and_no_timer_for_plain_keys() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    q.offer(&mut h, key("a"));
    q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.events, ["a", "^C"]);
    assert!(h.armed.is_empty());
    assert!(q.is_idle());
}

#[test]
fn helpers_report_gates_and_queued_bytes_without_content() {
    let mut q = InputQueue::new();
    let mut h = Recorder::default();
    assert!(!q.has_gate_kind(GateKind::PasteReading));
    start_paste(&mut q, &mut h, 4);
    q.offer(&mut h, key("ab"));
    assert!(q.has_gate(GateKind::PasteReading, 4));
    assert!(!q.has_gate(GateKind::PasteReading, 5));
    assert_eq!(q.queued_summary(), (2, 2));
}
