//! Interleaving tests of [`PaneHandoff`]: the RwLock handle shared by the key path (reads, from
//! the main thread) and the model thread (writes while it parses).
//!
//! ```text
//! RUSTFLAGS="--cfg shuttle" cargo test -p warp --lib --features warp/gui --release -- model_tests::model_
//! ```

use warpui_core::sync_shim::{check, thread};

use super::*;

/// A reader never sees the prompt counter go backwards or a prompt-sequence mode without a
/// sequence number, while the model thread parses Precmds and flips the kill switch. A nested
/// read (the handle's documented use) cannot deadlock against the writer.
#[test]
fn model_pane_handoff_reads_are_consistent_while_the_model_writes() {
    check(|| {
        let handoff = PaneHandoff::default();
        let model = {
            let handoff = handoff.clone();
            thread::spawn(move || {
                handoff.write().set_prompt_seq_enabled(true);
                for seq in 1..=3u64 {
                    handoff.write().on_precmd_seq(Some(7), Some(seq));
                }
            })
        };

        let mut last = 0;
        for _ in 0..3 {
            let outer = handoff.read();
            let inner = handoff.read();
            let seen = outer.last_prompt_seq().map_or(0, |prompt| prompt.seq);
            assert!(seen >= last, "prompt seq went backwards: {seen} < {last}");
            last = seen;
            // Both guards are the same state: no writer slipped between them.
            assert_eq!(inner.last_prompt_seq(), outer.last_prompt_seq());
            if outer.prompt_seq_mode() {
                assert!(outer.last_prompt_seq().is_some());
            }
        }

        model.join().unwrap();
        let end = handoff.read();
        assert!(end.prompt_seq_mode());
        assert_eq!(end.last_prompt_seq().map(|prompt| prompt.seq), Some(3));
    });
}

/// Two writers (the model thread and the key path's own bookkeeping) serialize: both updates
/// land, and the later `on_precmd_seq` computes `preexecs_before` from the earlier one.
#[test]
fn model_pane_handoff_writers_serialize() {
    check(|| {
        let handoff = PaneHandoff::default();
        let a = {
            let handoff = handoff.clone();
            thread::spawn(move || handoff.write().on_precmd_seq(Some(1), Some(1)))
        };
        let b = {
            let handoff = handoff.clone();
            thread::spawn(move || handoff.write().on_precmd_seq(Some(1), Some(3)))
        };
        a.join().unwrap();
        b.join().unwrap();
        let prompt = handoff
            .read()
            .last_prompt_seq()
            .expect("a Precmd was parsed");
        match prompt.seq {
            // 1 then 3: two numbers apart, so one Preexec in between (the counter counts them).
            3 => assert_eq!(prompt.preexecs_before, Some(1)),
            // 3 then 1: a lower number of the same shell is not an advance, so no gap is known.
            1 => assert_eq!(prompt.preexecs_before, None),
            other => panic!("impossible sequence number {other}"),
        }
    });
}
