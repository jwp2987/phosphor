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
