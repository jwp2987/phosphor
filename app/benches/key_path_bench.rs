//! Per-key cost of the input path's pure parts.
//!
//! ```text
//! cargo bench -p warp --features dev-support --bench key_path_bench
//! ```
//!
//! Budget (owner): the whole key-to-paint overhead Phosphor adds is <= 2 ms, Enter <= 2 ms, UI
//! work <= 4 ms per event. These benches measure the slice of that this code owns, so each must
//! be orders of magnitude below it: a key through an idle pane queue, the destination decision
//! and the hand-off decision should each be well under 10 microseconds (typically sub-
//! microsecond to a few microseconds). A result above 10 us per key is a regression worth
//! chasing; above 100 us the key path is eating the budget by itself.
//!
//! * `pane_queue/idle_key`: one key offered to an idle queue (decide the destination, deliver).
//! * `pane_queue/gate_open_offer_drain`: 64 keys offered behind an open gate, then the gate's
//!   event drains them (per-key cost = time / 64; the throughput line reports it).
//! * `destination_for`: the pure destination table over the common (kind, facts) pairs.
//! * `handoff`: the hand-off decision (`young_submit_decision`) and the lock-free reads a key
//!   makes of the model facts (`KeyFactsCell::load`, `PaneHandoff::read`).

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use warp::dev_support::{
    DeliverOutcome, DestFacts, Destination, DiscardReason, EntryKind, FollowUps, GateKind,
    GateSpec, GateToken, Host, InputEntry, PaneQueue, destination_for,
};
use warp::terminal::model::key_facts::{KeyFacts, KeyFactsCell};
use warp::terminal::model::typeahead_handoff::{PaneHandoff, young_submit_decision};

/// Delivers nothing anywhere: the queue's own cost, without a pty behind it.
#[derive(Default)]
struct NullHost {
    delivered: usize,
}

fn open_facts() -> DestFacts {
    DestFacts {
        pty_takes_input: true,
        input_box_visible: true,
        pty_open: true,
        ..DestFacts::default()
    }
}

impl Host for NullHost {
    fn facts(&self) -> DestFacts {
        open_facts()
    }

    fn deliver(
        &mut self,
        _entry: &InputEntry,
        _dest: Destination,
        _follow: &mut FollowUps,
    ) -> DeliverOutcome {
        self.delivered += 1;
        DeliverOutcome::Delivered
    }

    fn discarded(&mut self, _entry: &InputEntry, _why: DiscardReason) {}

    fn arm_give_up(&mut self, _token: GateToken, _kind: GateKind, _after: Duration) {}

    fn warn(&mut self, _line: &str) {}
}

fn key() -> EntryKind {
    EntryKind::PtyBytes {
        bytes: vec![b'a'],
        takes_over_agent_control: true,
    }
}

fn pane_queue(c: &mut Criterion) {
    let mut group = c.benchmark_group("pane_queue");

    let pane = PaneQueue::new();
    let mut host = NullHost::default();
    group.bench_function("idle_key", |b| {
        b.iter(|| {
            black_box(pane.lock().offer(&mut host, key()));
        })
    });
    assert!(host.delivered > 0, "the idle queue delivers inline");

    const KEYS: u64 = 64;
    group.throughput(Throughput::Elements(KEYS));
    let pane = PaneQueue::new();
    let mut host = NullHost::default();
    group.bench_function("gate_open_offer_drain", |b| {
        b.iter(|| {
            let mut queue = pane.lock();
            queue.open_gate(&mut host, GateSpec::new(GateKind::SshConnect, 1));
            for _ in 0..KEYS {
                queue.offer(&mut host, key());
            }
            black_box(queue.on_gate_event(&mut host, GateKind::SshConnect, 1));
        })
    });
    assert!(host.delivered > 0, "the gate's event drains what it held");
    group.finish();
}

fn destinations(c: &mut Criterion) {
    let facts = open_facts();
    let editor_focused = DestFacts {
        editor_focused: true,
        ..facts
    };
    let agent = DestFacts {
        agent_owns_pty: true,
        ..facts
    };
    let kinds = [
        EntryKind::Text("a".to_owned()),
        EntryKind::Enter,
        EntryKind::Control("backspace"),
        EntryKind::Raw(vec![0x1b, b'[']),
        key(),
    ];
    c.bench_function("destination_for", |b| {
        b.iter(|| {
            for kind in &kinds {
                for facts in [&facts, &editor_focused, &agent] {
                    black_box(destination_for(black_box(kind), black_box(facts)));
                }
            }
        })
    });
}

fn handoff(c: &mut Criterion) {
    let mut group = c.benchmark_group("handoff");
    group.bench_function("young_submit_decision", |b| {
        b.iter(|| {
            black_box(young_submit_decision(
                black_box(true),
                black_box(true),
                black_box(Duration::from_millis(40)),
            ))
        })
    });

    let cell = KeyFactsCell::default();
    cell.store(KeyFacts {
        shell_reports_line: true,
        ..KeyFacts::default()
    });
    group.bench_function("key_facts_load", |b| b.iter(|| black_box(cell.load())));

    let handoff = PaneHandoff::default();
    group.bench_function("pane_handoff_read", |b| {
        b.iter(|| black_box(handoff.read().prompt_seq_mode()))
    });
    group.finish();
}

criterion_group!(benches, pane_queue, destinations, handoff);
criterion_main!(benches);
