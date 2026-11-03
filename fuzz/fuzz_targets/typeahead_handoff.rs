//! The typeahead hand-off state machine (`TypeaheadHandoff`, #1403/#1422) under arbitrary event
//! sequences: shell hooks (precmd, preexec, line-editor signals), the editor's commands and
//! Enters, user bytes (ordinary and to a full-screen program), reports, interrupts, session end.
//! No event may panic, and the bookkeeping that keeps a key from being lost must hold after
//! every event:
//!
//! * a key answered `Held` has a live hold to carry it (a queued young hold, a queued prompt
//!   hold, or a deferred Enter's gate): the view puts the key behind that gate, so a `Held` with
//!   no hold would drop it;
//! * a signal byte is never held;
//! * `WriteCounted` is only ever answered while the pty owns input;
//! * a hold that STARTS on a key is reported to the view (`take_*_hold_started`), or the key that
//!   started it would never be queued behind its gate;
//! * a queued young hold that ENDS on a key, a report, a written command, a Preexec, the block
//!   latch, the barrier's publish, a request decision or the session end leaves an end for the
//!   view (`take_young_hold_ends`), or its gate would outlive the hold and strand the keys behind
//!   it;
//! * a young hold that starts names its evidence (#1614), and one that stands on "the request is
//!   due" has no line queued in the counts (its release, the report, is satisfiable);
//! * the session end clears every hold.
//!
//! The same state under real thread interleavings is the model in
//! `app/src/terminal/model/pipeline_model_tests.rs`.
#![no_main]

use std::sync::Arc;

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use warp::terminal::model::typeahead_handoff::{
    ForegroundProbe, HandoffState, ShellPromptFact, TypeaheadHandoff, UserBytesRoute,
    YoungHoldEvidence,
};

const SIGNAL_BYTES: [u8; 4] = [0x03, 0x04, 0x1a, 0x1c];

/// A small alphabet of key chunks: text, Enter, CRLF, the four signal bytes, a multi-byte write.
fn chunk(u: &mut Unstructured) -> Vec<u8> {
    match u.int_in_range(0..=8u8).unwrap_or(0) {
        0 => b"a".to_vec(),
        1 => b"ls".to_vec(),
        2 => b"\r".to_vec(),
        3 => b"\n".to_vec(),
        4 => b"\r\n".to_vec(),
        5 => vec![SIGNAL_BYTES[usize::from(u.arbitrary::<u8>().unwrap_or(0)) % 4]],
        6 => b"x\r".to_vec(),
        7 => b"\x1b[A".to_vec(),
        _ => b"echo hi\r".to_vec(),
    }
}

fn flag(u: &mut Unstructured) -> bool {
    u.arbitrary::<bool>().unwrap_or(false)
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let mut h = TypeaheadHandoff::default();
    // The ends the view has been told about (it takes them after every event).
    let mut ends_taken: usize = 0;

    for _ in 0..96 {
        let Ok(event) = u.int_in_range(0..=28u8) else {
            break;
        };
        let young_before = h.young_hold_queued();
        let prompt_before = h.prompt_hold_queued();
        let mut ends_expected = false;

        match event {
            0 | 1 | 2 => {
                let bytes = chunk(&mut u);
                let route = if event == 2 {
                    h.on_user_bytes_to_full_screen_program(&bytes)
                } else {
                    h.on_user_bytes(&bytes)
                };
                let live =
                    h.young_hold_queued() || h.prompt_hold_queued() || h.deferred_gate().is_some();
                match route {
                    UserBytesRoute::Held => {
                        assert!(live, "a key was held with no live hold: {bytes:?}");
                        assert!(
                            !bytes.iter().any(|byte| SIGNAL_BYTES.contains(byte)),
                            "a signal was held: {bytes:?}"
                        );
                    }
                    UserBytesRoute::WriteCounted => {
                        assert!(
                            matches!(h.state(), HandoffState::PtyOwned { .. }),
                            "a counted write outside PtyOwned: {:?}",
                            h.state()
                        );
                    }
                    UserBytesRoute::Write => {}
                }
                // A young hold that started on this key must be reported to the view, and it
                // names evidence whose release is satisfiable (#1614).
                if !young_before && h.young_hold_queued() {
                    assert!(
                        h.take_young_hold_started().is_some(),
                        "a young hold started and was not reported"
                    );
                    let evidence = h
                        .young_hold_evidence()
                        .expect("a young hold names its evidence");
                    if evidence == YoungHoldEvidence::RequestDue {
                        assert!(
                            !h.counts_diverged(),
                            "a hold for a request that is due has a line queued: {bytes:?}"
                        );
                    }
                }
                if !prompt_before && h.prompt_hold_queued() {
                    assert!(
                        h.take_prompt_hold_started().is_some(),
                        "a prompt hold started and was not reported"
                    );
                }
                // A young hold that ended on this key (a signal, a stale hold) left an end.
                ends_expected = young_before && !h.young_hold_queued();
            }
            3 => {
                h.on_precmd();
            }
            4 => {
                // #1614: a Preexec ends a live young hold as typeahead.
                let _ = h.on_preexec(flag(&mut u));
                ends_expected = young_before && !h.young_hold_queued();
            }
            5 => {
                h.on_command_written();
                ends_expected = young_before && !h.young_hold_queued();
            }
            6 => {
                let _ = h.on_report();
                ends_expected = young_before && !h.young_hold_queued();
            }
            7 => {
                let _ = h.release_after_unsubmitted_enter();
            }
            8 => {
                h.reset_on_exit();
                assert!(
                    !h.young_hold_queued()
                        && !h.prompt_hold_queued()
                        && h.deferred_gate().is_none(),
                    "the session end left a hold"
                );
                assert_eq!(h.state(), &HandoffState::EditorOwned);
                ends_expected = young_before && !h.young_hold_queued();
            }
            9 => {
                let _ = h.on_interrupt();
            }
            10 => {
                let _ = h.defer_enter();
            }
            11 => {
                let _ = h.cancel_deferred_enter();
            }
            12 => {
                let _ = h.adopt_untracked_typeahead();
            }
            13 => {
                // #1614: a decision that finds no request possible ends a live hold as typeahead.
                let at_prompt = flag(&mut u);
                let can_request = flag(&mut u);
                let _ = h.on_line_editor_active(at_prompt, can_request);
                ends_expected = young_before && !h.young_hold_queued();
            }
            14 => h.set_conpty_input(flag(&mut u)),
            15 => h.set_conpty_prompt_hold(flag(&mut u)),
            16 => h.set_conpty_prompt_hold_shell_signals(flag(&mut u)),
            17 => h.set_prompt_seq_enabled(flag(&mut u)),
            18 => {
                let session = u.arbitrary::<u8>().ok().map(u64::from);
                let seq = u.arbitrary::<u8>().ok().map(u64::from);
                h.on_precmd_seq(session, seq);
            }
            19 => {
                let session = u.arbitrary::<u8>().ok().map(u64::from);
                let seq = u.arbitrary::<u8>().ok().map(u64::from);
                let _ = h.on_line_editor_reading(session, seq);
            }
            20 => h.on_new_session(),
            21 => {
                let _ = h.on_user_write_flushed();
            }
            22 => h.note_interactive_command_finished(),
            23 => {
                // The line editor's signal releases a queued prompt hold, as the view does.
                if let Some(id) = h.release_queued_prompt_hold_on_signal() {
                    h.finish_queued_prompt_hold(id);
                }
            }
            24 => {
                let _ = h.check_queued_young_hold();
            }
            25 => {
                let _ = h.try_commit_at_prompt();
            }
            26 => {
                // #1614: the block latch says the command is interactive.
                h.note_command_interactive();
                ends_expected = young_before && !h.young_hold_queued();
            }
            27 => {
                // #1614: the pty reader applied a slice; a hold waiting on the barrier decides.
                let _ = h.on_stream_applied();
                ends_expected = young_before && !h.young_hold_queued();
            }
            _ => {
                // The platform's sample and barrier, as installed by the terminal manager: a fixed
                // answer per installation, replaced by the next one.
                let fact = match u.int_in_range(0..=2u8).unwrap_or(2) {
                    0 => ShellPromptFact::NotAtPrompt,
                    1 => ShellPromptFact::LineEditorReading,
                    _ => ShellPromptFact::Unknown,
                };
                let mut probe = ForegroundProbe::default().with_prompt_fact(Arc::new(move || fact));
                match u.int_in_range(0..=2u8).unwrap_or(0) {
                    0 => {}
                    barrier => {
                        let drained = barrier == 1;
                        probe = probe.with_stream_drained(Arc::new(move || drained));
                    }
                }
                h.set_foreground_probe(probe);
            }
        }

        // The view applies the ends after every event.
        let ends = h.take_young_hold_ends();
        if ends_expected {
            assert!(
                !ends.is_empty(),
                "event {event}: a queued young hold ended and left no end for its gate"
            );
        }
        ends_taken += ends.len();
    }
    // Keeps the counter observable so the accounting above is not optimised into nothing.
    std::hint::black_box(ends_taken);
});
