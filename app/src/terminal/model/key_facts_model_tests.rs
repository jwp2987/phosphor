//! Interleaving tests of the cross-thread key-path primitives: [`KeyFactsCell`], [`KeyModelCell`]
//! and [`ModelInbox`]. The writer side stands for the pty reader / model thread, the other side
//! for the main thread's user key.
//!
//! ```text
//! RUSTFLAGS="--cfg shuttle" cargo nextest run -p warp --lib --features warp/gui -E 'test(model_tests::)'
//! ```
//!
//! (or `cargo test -p warp --lib --features warp/gui --release -- model_tests::`; the filter matters: the rest of the
//! suite runs real primitives outside a model.)

use warpui_core::sync_shim::{Arc, check, thread};

use super::*;

const FACTS: KeyFacts = KeyFacts {
    alt_screen: true,
    agent_in_control: false,
    received_user_input: false,
    command_not_owned_by_handoff_possible: true,
    shell_reports_line: true,
};

/// A key that loads while the model publishes sees one published value whole, never a mix, and
/// a `note_received_user_input` racing a `store` ends as either order of the two.
#[test]
fn model_key_facts_publish_and_note_never_tear() {
    check(|| {
        let cell = KeyFactsCell::default();
        let publisher = {
            let cell = cell.clone();
            thread::spawn(move || cell.store(FACTS))
        };
        let noter = {
            let cell = cell.clone();
            thread::spawn(move || cell.note_received_user_input())
        };

        let seen = cell.load();
        // Whatever it saw is a state some interleaving produces.
        let noted = KeyFacts {
            received_user_input: true,
            ..FACTS
        };
        let initial = KeyFacts::default();
        let initial_noted = KeyFacts {
            received_user_input: true,
            ..initial
        };
        assert!(
            [initial, initial_noted, FACTS, noted].contains(&seen),
            "torn read: {seen:?}"
        );

        publisher.join().unwrap();
        noter.join().unwrap();
        let end = cell.load();
        // The store wrote FACTS (input not yet received); the note may land before or after it.
        assert!([FACTS, noted].contains(&end), "end state: {end:?}");
    });
}

/// The snapshot cell hands out whole snapshots: a loader never sees a store half done.
#[test]
fn model_key_model_snapshot_is_never_torn() {
    check(|| {
        let cell = KeyModelCell::default();
        let model = {
            let cell = cell.clone();
            thread::spawn(move || {
                for generation in 1..=2u64 {
                    cell.store(KeyModelSnapshot {
                        active_block_hash: generation,
                        active_block_started: true,
                        has_pending_ssh_session: generation == 2,
                        ..KeyModelSnapshot::default()
                    });
                }
            })
        };
        let seen = cell.load();
        // `active_block_started` and `active_block_hash` were stored together.
        assert_eq!(seen.active_block_started, seen.active_block_hash != 0);
        assert_eq!(seen.has_pending_ssh_session, seen.active_block_hash == 2);
        model.join().unwrap();
    });
}

/// The inbox loses nothing and reorders nothing per poster, with the reader draining while two
/// posters post: the model applies the key's records in the order the key left them.
#[test]
fn model_inbox_keeps_every_command_in_order_per_poster() {
    check(|| {
        let inbox = ModelInbox::default();
        let mut posters = Vec::new();
        for poster in 0..2u8 {
            let inbox = inbox.clone();
            posters.push(thread::spawn(move || {
                for n in 0..2u8 {
                    inbox.post(ModelCommand::TypedAhead(vec![poster, n]));
                }
            }));
        }

        // The pty reader: drains while the posters run.
        let mut drained = inbox.take();
        for poster in posters {
            poster.join().unwrap();
        }
        // Everything posted is either drained already or still waiting.
        drained.extend(inbox.take());
        assert!(!inbox.has_pending() || inbox.take().is_empty());

        assert_eq!(drained.len(), 4, "a command was lost: {drained:?}");
        for poster in 0..2u8 {
            let mine: Vec<u8> = drained
                .iter()
                .filter_map(|command| match command {
                    ModelCommand::TypedAhead(bytes) if bytes[0] == poster => Some(bytes[1]),
                    _ => None,
                })
                .collect();
            assert_eq!(mine, vec![0, 1], "poster {poster} reordered: {drained:?}");
        }
    });
}

#[test]
fn model_arc_is_the_checkers() {
    // `Arc` here is the checker's (leak-tracked under loom); keeps the import honest.
    check(|| {
        let a = Arc::new(KeyFactsCell::default());
        assert_eq!(a.load(), KeyFacts::default());
    });
}

// ---- the two-cell gen and the stamp (#1424) -----------------------------------------------

/// What `TerminalModel::publish_key_facts` does, in its order: the snapshot first, then the facts,
/// both under the new gen. The content is chosen so a mixed read is visible: the facts say
/// `alt_screen` exactly for gen 2, the snapshot says `active_block_started` exactly for gen 2.
fn publish(facts: &KeyFactsCell, model: &KeyModelCell, generation: u64, unterminated: bool) {
    model.store(KeyModelSnapshot {
        facts_gen: generation,
        active_block_started: generation == 2,
        unterminated_since_gen: unterminated.then_some(generation),
        ..KeyModelSnapshot::default()
    });
    facts.store_with_gen(
        KeyFacts {
            alt_screen: generation == 2,
            ..KeyFacts::default()
        },
        generation,
    );
}

/// A reader that races two publications never combines the facts of one with the snapshot of
/// another when it reports a consistent read, and a torn read reports the older gen.
#[test]
fn model_the_reader_retry_never_combines_gens() {
    check(|| {
        let facts = KeyFactsCell::default();
        let model = KeyModelCell::default();
        let publisher = {
            let (facts, model) = (facts.clone(), model.clone());
            thread::spawn(move || {
                publish(&facts, &model, 1, false);
                publish(&facts, &model, 2, false);
            })
        };
        let read = load_consistent_with(&facts, &model, 2);
        if read.consistent {
            assert_eq!(
                read.facts.alt_screen, read.snapshot.active_block_started,
                "facts of one publication under the snapshot of another: {read:?}"
            );
            assert_eq!(read.snapshot.facts_gen == 2, read.facts.alt_screen);
            assert_eq!(read.facts_gen, read.snapshot.facts_gen);
        } else {
            assert_eq!(read.facts_gen, read.snapshot.facts_gen.saturating_sub(1));
        }
        publisher.join().unwrap();
    });
}

/// The gen a reader reports is monotonic across loads, and never newer than the gen loaded after
/// it: a stamp is never from the future.
#[test]
fn model_the_gen_is_monotonic_across_loads() {
    check(|| {
        let facts = KeyFactsCell::default();
        let model = KeyModelCell::default();
        let publisher = {
            let (facts, model) = (facts.clone(), model.clone());
            thread::spawn(move || {
                publish(&facts, &model, 1, false);
                publish(&facts, &model, 2, false);
            })
        };
        let (first, _) = load_gen_consistent_with(&facts, &model, 2);
        let (second, _) = load_gen_consistent_with(&facts, &model, 2);
        assert!(second >= first, "gen went back: {first} then {second}");
        // Whatever it stamped has been published (the cell's own gen is at least as new).
        assert!(first <= model.facts_gen());
        publisher.join().unwrap();
    });
}

/// Stamp versus publish (H4, OD-7): a key whose stamp is taken AFTER the reader has seen the
/// prompt in a consistent read is never classed before the prompt, in any interleaving; a key
/// stamped before may be (it was typed before the prompt existed).
#[test]
fn model_a_stamp_taken_after_seeing_the_prompt_is_not_before_it() {
    check(|| {
        let facts = KeyFactsCell::default();
        let model = KeyModelCell::default();
        let publisher = {
            let (facts, model) = (facts.clone(), model.clone());
            thread::spawn(move || {
                publish(&facts, &model, 1, false);
                // The prompt text appears in this publication.
                publish(&facts, &model, 2, true);
            })
        };
        let (early_stamp, _) = load_gen_consistent_with(&facts, &model, 2);
        let seen = load_consistent_with(&facts, &model, 2);
        if let (true, Some(prompt_gen)) = (seen.consistent, seen.snapshot.unterminated_since_gen) {
            let (late_stamp, _) = load_gen_consistent_with(&facts, &model, 2);
            assert!(
                late_stamp >= prompt_gen,
                "a key stamped after the prompt was seen was classed before it: {late_stamp} < {prompt_gen}"
            );
            assert!(early_stamp <= late_stamp);
        }
        publisher.join().unwrap();
    });
}
