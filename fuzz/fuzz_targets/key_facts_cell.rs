//! The key path's published cells (#1424 design rev2 section 5) under arbitrary single-threaded
//! operation sequences, against a plain reference model:
//!
//! * `KeyFactsCell::store` (a main-thread refresh) changes the bits and never the gen;
//! * `store_with_gen` (the model's publication) sets both, the gen's low 32 bits only;
//! * `note_received_user_input` sets one bit and touches nothing else;
//! * `load_with_gen` always returns the reference facts and gen;
//! * the reader (`load_consistent`, `load_gen_consistent`) reports a consistent pair exactly when
//!   the low 32 bits of the two gens agree, with the snapshot's gen; a torn pair reports the
//!   snapshot's gen minus one (never newer than a publication), and never panics;
//! * a pair published in the model's order (snapshot, then facts, same gen) always reads
//!   consistent, whatever came before.
//!
//! The interleavings of the same cells across threads are the shuttle/loom models in
//! `app/src/terminal/model/{key_facts,pipeline}_model_tests.rs`.
#![no_main]

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use warp::terminal::model::key_facts::{
    KeyFacts, KeyFactsCell, KeyModelCell, KeyModelSnapshot, load_consistent, load_consistent_with,
    load_gen_consistent, load_gen_consistent_with,
};

const LOW32: u64 = 0xFFFF_FFFF;

fn facts_from(bits: u8) -> KeyFacts {
    KeyFacts {
        alt_screen: bits & 1 != 0,
        agent_in_control: bits & 2 != 0,
        received_user_input: bits & 4 != 0,
        command_not_owned_by_handoff_possible: bits & 8 != 0,
        shell_reports_line: bits & 16 != 0,
    }
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let facts_cell = KeyFactsCell::default();
    let model_cell = KeyModelCell::default();

    // The reference model.
    let mut facts = KeyFacts::default();
    let mut facts_gen: u64 = 0;
    let mut snapshot_gen: u64 = 0;

    for _ in 0..64 {
        let Ok(op) = u.int_in_range(0..=6u8) else {
            break;
        };
        let bits = u.arbitrary::<u8>().unwrap_or(0);
        let generation = u.arbitrary::<u64>().unwrap_or(0);
        match op {
            // A main-thread refresh: the bits, under the gen the cell holds.
            0 => {
                facts = facts_from(bits);
                facts_cell.store(facts);
            }
            // A publication of the facts alone.
            1 => {
                facts = facts_from(bits);
                facts_gen = generation & LOW32;
                facts_cell.store_with_gen(facts, generation);
            }
            // A publication of the snapshot alone (the first half of the model's publish).
            2 => {
                snapshot_gen = generation;
                model_cell.store(KeyModelSnapshot {
                    facts_gen: generation,
                    ..KeyModelSnapshot::default()
                });
            }
            // The model's publish, in its order: snapshot first, then facts, one gen.
            3 => {
                facts = facts_from(bits);
                facts_gen = generation & LOW32;
                snapshot_gen = generation;
                model_cell.store(KeyModelSnapshot {
                    facts_gen: generation,
                    ..KeyModelSnapshot::default()
                });
                facts_cell.store_with_gen(facts, generation);

                let read = load_consistent(&facts_cell, &model_cell);
                assert!(read.consistent, "a pair published in order read torn");
                assert_eq!(read.facts_gen, generation);
                assert_eq!(read.facts, facts);
                assert_eq!(
                    load_gen_consistent(&facts_cell, &model_cell),
                    (generation, true)
                );
            }
            // The first key of the block marks it.
            4 => {
                facts.received_user_input = true;
                facts_cell.note_received_user_input();
            }
            // A reader, with a random number of attempts.
            5 => {
                let attempts = usize::from(bits % 10);
                let read = load_consistent_with(&facts_cell, &model_cell, attempts);
                let agree = (snapshot_gen & LOW32) == facts_gen;
                assert_eq!(read.consistent, agree, "consistency flag");
                if agree {
                    assert_eq!(read.facts_gen, snapshot_gen);
                } else {
                    assert_eq!(read.facts_gen, snapshot_gen.saturating_sub(1));
                }
                assert_eq!(read.facts, facts);
                let (stamp, consistent) =
                    load_gen_consistent_with(&facts_cell, &model_cell, attempts);
                assert_eq!(consistent, agree);
                assert_eq!(
                    stamp,
                    if agree {
                        snapshot_gen
                    } else {
                        snapshot_gen.saturating_sub(1)
                    }
                );
            }
            // A plain load.
            _ => {}
        }
        let (loaded, loaded_gen) = facts_cell.load_with_gen();
        assert_eq!(loaded, facts, "facts after op {op}");
        assert_eq!(u64::from(loaded_gen), facts_gen, "gen after op {op}");
        assert_eq!(facts_cell.load(), facts);
        assert_eq!(model_cell.facts_gen(), snapshot_gen);
    }
});
