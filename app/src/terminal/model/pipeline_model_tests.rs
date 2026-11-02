//! Interleaving models of the input pipeline (#1512, #1531, #1424): the pane queue's drain against
//! the deferred mailbox, the ssh connect hand-over (#1348), the typeahead hand-off's hold
//! lifecycle, and the two-cell facts gen. They complement `key_facts_model_tests.rs` (the cells
//! and the reader retry) and `typeahead_handoff_model_tests.rs` (the `RwLock` handle).
//!
//! ```text
//! RUSTFLAGS="--cfg shuttle" cargo test -p warp --lib --features warp/gui --release -- model_tests::
//! RUSTFLAGS="--cfg loom"    cargo test -p warp --lib --features warp/gui --release -- model_tests::
//! ```
//!
//! The module name ends in `model_tests`, so the `test-shuttle-loom` job's `model_tests::` filter
//! selects it with no change to the job.
//!
//! WHAT IS A MODEL HERE. `PaneQueue` is an `Rc` and `InputQueue` holds `Rc` payloads: neither is
//! `Send`, and every real call runs on the main thread. The interleavings that matter are
//! therefore not preemptions but WHERE another operation can run between two steps of one:
//! inside a drain's delivery (a key that arrives then finds the queue busy), between "the drain
//! ended" and "the view applies the mailbox", between the hand-over closing the ssh gate and its
//! drain finishing. Each model below turns every such point into a lock release, so the checker
//! schedules another actor there. A model is a faithful minimal copy of the protocol, with the
//! code it mirrors named in its comment; the hand-off test uses the real `TypeaheadHandoff`
//! (it is `Send + Sync` behind `PaneHandoff`) and the gen test the real cells.
//!
//! The owner's invariants, asserted by every model: no key is lost, duplicated or reordered, and
//! no key is delivered to two destinations.
//!
//! Loom explores every schedule, so its bodies are sized down with `cfg!(loom)`.

use std::collections::VecDeque;

use warpui_core::sync_shim::{Arc, Mutex, check, thread};

use crate::terminal::model::key_facts::{
    KeyFacts, KeyFactsCell, KeyModelCell, KeyModelSnapshot, load_consistent_with,
    load_gen_consistent_with,
};
use crate::terminal::model::typeahead_handoff::{PaneHandoff, UserBytesRoute};

/// Keys per producer: loom walks every schedule, shuttle samples them.
fn keys_per_producer() -> u32 {
    if cfg!(loom) { 1 } else { 2 }
}

// ---- 1. the pane queue: drain versus the deferred mailbox ---------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dest {
    Pty,
    Editor,
}

/// The pane queue's shared protocol, one lock section per step the real code can be interrupted
/// at.
///
/// Mirrors `app/src/terminal/input_queue/pane.rs`: `draining` is `Inner::draining` (pane.rs:142; true while a
/// `QueueGuard` lives, `try_lock` pane.rs:291), `deferred` is `Inner::deferred` (`defer` pane.rs:231,
/// `pop_deferred` pane.rs:249), `flushing` is `flushing_deferred` (`begin_deferred_flush`
/// pane.rs:259). The arrival rule is `try_lock_ordered` (pane.rs:312): no lock while earlier
/// operations wait in the mailbox, so a new arrival can never overtake them.
#[derive(Default)]
struct PaneModel {
    draining: bool,
    flushing: bool,
    /// Entries inside the queue, delivered by the drain in order.
    queue: VecDeque<u32>,
    /// Operations that found the queue busy, in the order they were recorded.
    deferred: VecDeque<u32>,
    /// Keys in the order their sink saw them: the order they must be delivered in.
    arrivals: Vec<u32>,
    /// The destination is decided when the key is DELIVERED, from a fact a third actor flips.
    pty_takes: bool,
    delivered: Vec<(u32, Dest)>,
    next_key: u32,
}

type Pane = Arc<Mutex<PaneModel>>;

/// A key reaches the sink (`on_pane_key_event`): its arrival position is taken, then
/// `try_lock_ordered`: the queue is taken if it is free and no earlier operation waits, else the
/// key is deferred. Taking the queue and offering the key are one step (one guard).
fn arrive(pane: &Pane) {
    {
        let mut p = pane.lock();
        let key = p.next_key;
        p.next_key += 1;
        p.arrivals.push(key);
        if p.draining || !p.deferred.is_empty() {
            p.deferred.push_back(key);
            return;
        }
        p.draining = true;
        p.queue.push_back(key);
    }
    drain(pane);
    flush(pane);
}

/// The drain: delivers the queue's entries in order, one step each, and releases the queue when
/// it is empty (the `QueueGuard` drop, pane.rs:368).
fn drain(pane: &Pane) {
    loop {
        let mut p = pane.lock();
        match p.queue.pop_front() {
            Some(key) => {
                let dest = if p.pty_takes { Dest::Pty } else { Dest::Editor };
                p.delivered.push((key, dest));
            }
            None => {
                p.draining = false;
                return;
            }
        }
    }
}

/// `TerminalView::flush_deferred_queue_ops` (view/paste_queue.rs:1954): applies the mailbox in recorded order the moment the
/// view is outside any operation. Claims the flush (`begin_deferred_flush`); a flush that finds
/// the queue busy leaves the operation where it is (`unpop_deferred`) and ends: the drain that is
/// running calls this again when it finishes (the wake-up), so nothing waits for a later key.
fn flush(pane: &Pane) {
    {
        let mut p = pane.lock();
        if p.flushing {
            return;
        }
        p.flushing = true;
    }
    loop {
        {
            let mut p = pane.lock();
            if p.draining {
                p.flushing = false;
                return;
            }
            match p.deferred.pop_front() {
                Some(key) => {
                    p.draining = true;
                    p.queue.push_back(key);
                }
                None => {
                    p.flushing = false;
                    return;
                }
            }
        }
        drain(pane);
    }
}

/// Two typing routes (the terminal element and the editor) and a flush race a focus flip that
/// changes where a delivered key goes. Whatever the schedule, every key is delivered exactly
/// once, in the order it arrived, to one destination, and nothing is left waiting.
#[test]
fn model_pane_queue_drain_never_loses_reorders_or_duplicates_a_key() {
    check(|| {
        let pane: Pane = Arc::new(Mutex::new(PaneModel::default()));
        pane.lock().pty_takes = true;

        let mut handles = Vec::new();
        for _producer in 0..2 {
            let pane = pane.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..keys_per_producer() {
                    arrive(&pane);
                }
            }));
        }
        if !cfg!(loom) {
            let flipper = pane.clone();
            handles.push(thread::spawn(move || {
                flipper.lock().pty_takes = false;
            }));
            let waker = pane.clone();
            handles.push(thread::spawn(move || flush(&waker)));
        } else {
            flush(&pane);
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let p = pane.lock();
        assert!(!p.draining && !p.flushing, "the queue was left held");
        assert!(
            p.queue.is_empty() && p.deferred.is_empty(),
            "a key was left waiting: queue {:?}, mailbox {:?}",
            p.queue,
            p.deferred
        );
        let order: Vec<u32> = p.delivered.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            order, p.arrivals,
            "delivered out of arrival order, lost or duplicated (arrival order {:?}, delivered {:?})",
            p.arrivals, p.delivered
        );
    });
}

// ---- 2. the ssh connect hand-over (#1348) -------------------------------------------------------

/// The ssh hold on the one queue: keys typed between the Enter on `ssh ...` and the remote
/// shell's first prompt are entries behind an `SshConnect` gate.
///
/// Mirrors `app/src/terminal/view/ssh_queue.rs`: a key joins the hold while the gate is open
/// (`ssh_hold_push`, ssh_queue.rs:125); the hand-over (`deliver_ssh_typeahead_if_ready`, view.rs:11122) CLOSES
/// the gate and the queue's one drain writes the held keys in typed order, the close being the
/// delivery; a host-key / password prompt or a ^C DROPS the gate with everything held
/// (`reset_ssh_typeahead`, view.rs:11106); after either, a key that arrives while the drain still runs queues
/// behind what is left (the busy queue defers it, pane.rs `try_lock_ordered`), and one that arrives
/// later passes straight through.
#[derive(Default)]
struct SshModel {
    gate_open: bool,
    /// The hand-over drain is running (the queue is busy).
    draining: bool,
    queue: VecDeque<u32>,
    typed: Vec<u32>,
    /// Where a delivered key goes: the editor until the connect is settled, then the remote
    /// shell's pty (the prompt that wants an answer, or the hand-over). Decided at delivery.
    settled: bool,
    delivered: Vec<(u32, Dest)>,
    discarded: Vec<u32>,
    next_key: u32,
}

type Ssh = Arc<Mutex<SshModel>>;

fn ssh_deliver(s: &mut SshModel, key: u32) {
    assert!(!s.gate_open, "key {key} was delivered behind an open gate");
    let dest = if s.settled { Dest::Pty } else { Dest::Editor };
    s.delivered.push((key, dest));
}

/// A key typed during and after the connect.
fn ssh_type_key(ssh: &Ssh) {
    let mut s = ssh.lock();
    let key = s.next_key;
    s.next_key += 1;
    s.typed.push(key);
    if s.gate_open || s.draining || !s.queue.is_empty() {
        // Held behind the gate, or behind what the hand-over drain has not delivered yet.
        s.queue.push_back(key);
    } else {
        ssh_deliver(&mut s, key);
    }
}

/// The remote shell's first prompt: the single hand-over point. Closing the gate is one step,
/// each delivery of the drain another.
fn ssh_hand_over(ssh: &Ssh) {
    {
        let mut s = ssh.lock();
        if !s.gate_open {
            // Already dropped (or never opened): the hand-over finds no gate and does nothing.
            return;
        }
        s.gate_open = false;
        s.settled = true;
        s.draining = true;
    }
    loop {
        let mut s = ssh.lock();
        match s.queue.pop_front() {
            Some(key) => ssh_deliver(&mut s, key),
            None => {
                s.draining = false;
                return;
            }
        }
    }
}

/// A password / host-key prompt or a ^C: the gate and everything held behind it go, in one step.
fn ssh_drop_gate(ssh: &Ssh) {
    let mut s = ssh.lock();
    if !s.gate_open {
        return;
    }
    s.gate_open = false;
    s.settled = true;
    let held: Vec<u32> = s.queue.drain(..).collect();
    s.discarded.extend(held);
}

/// Held keys, the gate release and a new key arriving: each typed key ends in exactly one place
/// (delivered once, or discarded because the connect was dropped while it was held), the
/// delivered ones in typing order, and nothing is delivered while the gate is open.
#[test]
fn model_ssh_hand_over_keeps_every_held_key_in_order_and_delivers_it_once() {
    check(|| {
        let ssh: Ssh = Arc::new(Mutex::new(SshModel::default()));
        ssh.lock().gate_open = true;

        let typist = {
            let ssh = ssh.clone();
            thread::spawn(move || {
                for _ in 0..(keys_per_producer() + 1) {
                    ssh_type_key(&ssh);
                }
            })
        };
        let hand_over = {
            let ssh = ssh.clone();
            thread::spawn(move || ssh_hand_over(&ssh))
        };
        let dropper = if cfg!(loom) {
            None
        } else {
            let ssh = ssh.clone();
            Some(thread::spawn(move || ssh_drop_gate(&ssh)))
        };
        typist.join().unwrap();
        hand_over.join().unwrap();
        if let Some(dropper) = dropper {
            dropper.join().unwrap();
        }

        let s = ssh.lock();
        assert!(!s.draining, "the hand-over drain never finished");
        assert!(
            s.queue.is_empty(),
            "a held key was left behind: {:?}",
            s.queue
        );

        // Every typed key has exactly one fate.
        let mut fates: Vec<u32> = s.delivered.iter().map(|(key, _)| *key).collect();
        fates.extend(s.discarded.iter().copied());
        fates.sort_unstable();
        assert_eq!(
            fates, s.typed,
            "a key was lost or counted twice: {:?} / {:?}",
            s.delivered, s.discarded
        );

        // The delivered keys are in typing order: nothing overtook a held key.
        let order: Vec<u32> = s.delivered.iter().map(|(key, _)| *key).collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(
            order, sorted,
            "delivered out of typing order: {:?}",
            s.delivered
        );

        // Only keys held at the drop were discarded: none typed after the gate was gone, so a
        // discarded key is older than every key delivered after the drop. (A delivered key older
        // than a discarded one was delivered by the hand-over before the drop took the rest, which
        // the hand-over's single close step rules out.)
        if let (Some(first_discarded), Some(last_discarded)) =
            (s.discarded.iter().min(), s.discarded.iter().max())
        {
            assert!(
                s.delivered
                    .iter()
                    .all(|(key, _)| key < first_discarded || key > last_discarded),
                "a key was delivered between discarded keys: {:?} / {:?}",
                s.delivered,
                s.discarded
            );
        }
    });
}

// ---- 3. the hand-off's holds (young hold, deferred Enter) --------------------------------------

/// The key path against the model thread and an editor submit, on the REAL hand-off:
///
/// * a key the hand-off routes as `Held` is only ever answered while a hold is live (a queued
///   young hold, a queued prompt hold, or a deferred Enter's gate): the view puts it behind that
///   gate, so a `Held` with no live hold would be a lost key;
/// * a signal byte is never held;
/// * every hold the key path saw start (`take_young_hold_started`) ends, exactly the session end
///   at the latest, with an end recorded for the view to apply to the gate
///   (`take_young_hold_ends`): no gate outlives its hold and no key is stranded behind one.
///
/// The Enter's young-hold decision may depend on the clock (`YOUNG_ENTER_WINDOW`) when no prompt
/// sequence number is known; both answers are allowed, which is why the assertions are about the
/// lifecycle and not about which one was given.
#[test]
fn model_handoff_holds_start_and_end_once_and_never_strand_a_key() {
    check(|| {
        let handoff = PaneHandoff::default();
        {
            let mut h = handoff.write();
            h.set_prompt_seq_enabled(true);
            h.on_command_written();
            let _ = h.on_user_bytes(b"x");
        }

        let model = {
            let handoff = handoff.clone();
            thread::spawn(move || {
                handoff.write().on_precmd();
                handoff.write().on_precmd();
            })
        };
        let editor = {
            let handoff = handoff.clone();
            thread::spawn(move || {
                // The editor's command was accepted and written: a hold behind an Enter follows it.
                handoff.write().on_command_written();
            })
        };

        let mut started: Vec<u64> = Vec::new();
        let keys: [&[u8]; 3] = [b"\r", b"y", &[0x03]];
        for bytes in keys {
            let mut h = handoff.write();
            let route = h.on_user_bytes(bytes);
            let live =
                h.young_hold_queued() || h.prompt_hold_queued() || h.deferred_gate().is_some();
            if route == UserBytesRoute::Held {
                assert!(
                    live,
                    "a key was held with no live hold to carry it: {bytes:?}"
                );
            }
            if bytes.contains(&0x03) {
                assert_ne!(route, UserBytesRoute::Held, "a signal was held");
            }
            if let Some(id) = h.take_young_hold_started() {
                if !started.contains(&id) {
                    started.push(id);
                }
            }
        }

        model.join().unwrap();
        editor.join().unwrap();

        // The session ends: nothing can answer a hold any more.
        let mut h = handoff.write();
        h.reset_on_exit();
        let ends: Vec<u64> = h
            .take_young_hold_ends()
            .into_iter()
            .map(|(id, _end)| id)
            .collect();
        for id in &started {
            assert!(
                ends.contains(id),
                "hold {id} started and never ended (ends: {ends:?})"
            );
        }
        assert!(!h.young_hold_queued() && !h.prompt_hold_queued());
    });
}

// ---- 4. the two-cell gen: stamp, then decide ------------------------------------------------------

/// What `TerminalModel::publish_key_facts` does, in its order: the snapshot first, then the
/// facts, both under the new gen.
fn publish(facts: &KeyFactsCell, model: &KeyModelCell, generation: u64) {
    model.store(KeyModelSnapshot {
        facts_gen: generation,
        ..KeyModelSnapshot::default()
    });
    facts.store_with_gen(KeyFacts::default(), generation);
}

/// A main-thread refresh (`store`) racing a publication never invents or loses a gen: the cell's
/// gen is 0 or the published one at every load, and the published one at the end.
#[test]
fn model_a_refresh_racing_a_publication_never_moves_the_gen() {
    check(|| {
        let cell = KeyFactsCell::default();
        let publisher = {
            let cell = cell.clone();
            thread::spawn(move || {
                cell.store_with_gen(
                    KeyFacts {
                        alt_screen: true,
                        ..KeyFacts::default()
                    },
                    1,
                );
            })
        };
        let refresher = {
            let cell = cell.clone();
            thread::spawn(move || cell.store(KeyFacts::default()))
        };
        let (_, seen) = cell.load_with_gen();
        assert!(seen == 0 || seen == 1, "a gen nobody published: {seen}");
        publisher.join().unwrap();
        refresher.join().unwrap();
        let (_, end) = cell.load_with_gen();
        assert_eq!(end, 1, "the refresh lost the publication's gen");
    });
}

/// Stamp-then-decide (design rev2 section 5): the sink takes its stamp BEFORE the facts of the
/// decision are read, so the decision's gen is never older than the stamp's by more than the one
/// publication a torn read reports behind; and a consistent decision read is never older at all.
/// Also: the gen the reader reports is never newer than what has been published.
#[test]
fn model_the_decision_is_never_older_than_the_stamp_it_follows() {
    check(|| {
        let facts = KeyFactsCell::default();
        let model = KeyModelCell::default();
        let publisher = {
            let (facts, model) = (facts.clone(), model.clone());
            thread::spawn(move || {
                publish(&facts, &model, 1);
                publish(&facts, &model, 2);
                if !cfg!(loom) {
                    publish(&facts, &model, 3);
                }
            })
        };
        let (stamp, _) = load_gen_consistent_with(&facts, &model, 2);
        let decision = load_consistent_with(&facts, &model, 2);
        if decision.consistent {
            assert!(
                decision.facts_gen >= stamp,
                "a decision read ({}) older than the stamp before it ({stamp})",
                decision.facts_gen
            );
        } else {
            assert!(decision.facts_gen + 1 >= stamp);
        }
        assert!(decision.facts_gen <= model.facts_gen());
        publisher.join().unwrap();
    });
}
