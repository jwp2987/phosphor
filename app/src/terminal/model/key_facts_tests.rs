use proptest::collection::vec;
use proptest::prelude::{Just, ProptestConfig, Strategy, any, prop_oneof, proptest};

use super::*;

fn every_facts() -> impl Iterator<Item = KeyFacts> {
    (0u32..32).map(|bits| KeyFacts {
        alt_screen: bits & 1 != 0,
        agent_in_control: bits & 2 != 0,
        received_user_input: bits & 4 != 0,
        command_not_owned_by_handoff_possible: bits & 8 != 0,
        shell_reports_line: bits & 16 != 0,
    })
}

#[test]
fn every_combination_of_facts_survives_the_cell() {
    let cell = KeyFactsCell::default();
    assert_eq!(cell.load(), KeyFacts::default());
    for facts in every_facts() {
        cell.store(facts);
        assert_eq!(cell.load(), facts);
    }
}

#[test]
fn the_first_input_bit_is_set_by_the_key_path_without_touching_the_others() {
    let cell = KeyFactsCell::default();
    let facts = KeyFacts {
        alt_screen: true,
        shell_reports_line: true,
        ..KeyFacts::default()
    };
    cell.store(facts);
    cell.note_received_user_input();
    assert_eq!(
        cell.load(),
        KeyFacts {
            received_user_input: true,
            ..facts
        }
    );
}

#[test]
fn keys_reach_a_command_the_handoff_does_not_own_needs_both_facts() {
    for facts in every_facts() {
        assert_eq!(
            facts.keys_reach_a_command_the_handoff_does_not_own(),
            facts.shell_reports_line && facts.command_not_owned_by_handoff_possible
        );
    }
}

#[test]
fn the_inbox_hands_commands_over_in_posting_order_exactly_once() {
    let inbox = ModelInbox::default();
    assert!(!inbox.has_pending());
    assert!(inbox.take().is_empty());
    inbox.post(ModelCommand::TypedAhead(b"ls".to_vec()));
    inbox.post(ModelCommand::UserWrote {
        first_input: true,
        ctrl_c: false,
    });
    let clone = inbox.clone();
    clone.post(ModelCommand::TypedAhead(b"\r".to_vec()));
    assert!(inbox.has_pending());
    assert_eq!(
        inbox.take(),
        vec![
            ModelCommand::TypedAhead(b"ls".to_vec()),
            ModelCommand::UserWrote {
                first_input: true,
                ctrl_c: false
            },
            ModelCommand::TypedAhead(b"\r".to_vec()),
        ]
    );
    assert!(!inbox.has_pending());
    assert!(clone.take().is_empty(), "taken once");
}

// ---- the model side -----------------------------------------------------------------------

use crate::terminal::model::terminal_model::TerminalModel;

/// What the event loop does around every parse slice: apply the inbox, parse, publish.
fn parse_slice_like_the_event_loop(model: &mut TerminalModel, bytes: &str) {
    model.apply_inbox();
    model.process_bytes(bytes);
    model.publish_key_facts();
}

#[test]
fn a_parse_publishes_the_alt_screen_the_key_path_reads() {
    let mut model = TerminalModel::mock(None, None);
    let cell = model.key_facts_cell();
    model.publish_key_facts();
    assert!(!cell.load().alt_screen);
    parse_slice_like_the_event_loop(&mut model, "\x1b[?1049h");
    assert!(
        cell.load().alt_screen,
        "vim must get its keys, not the editor"
    );
    parse_slice_like_the_event_loop(&mut model, "\x1b[?1049l");
    assert!(!cell.load().alt_screen);
}

#[test]
fn the_inbox_marks_a_running_block_once_and_leaves_an_idle_one_alone() {
    let mut model = TerminalModel::mock(None, None);
    let inbox = model.model_inbox();
    // An idle block takes no input mark, but a ^C is recorded either way (#1178).
    inbox.post(ModelCommand::UserWrote {
        first_input: true,
        ctrl_c: false,
    });
    model.apply_inbox();
    assert!(!model.block_list().active_block().has_received_user_input());

    model.block_list_mut().active_block_mut().start();
    model.simulate_cmd("sleep 9");
    model
        .block_list_mut()
        .active_block_mut()
        .set_was_long_running(true.into());
    inbox.post(ModelCommand::UserWrote {
        first_input: true,
        ctrl_c: true,
    });
    model.apply_inbox();
    let block = model.block_list().active_block();
    assert!(block.has_received_user_input());
    assert!(block.was_interrupted_by_ctrl_c());
    // Applying publishes: the key path's next read sees the mark.
    assert!(model.key_facts_cell().load().received_user_input);
}

/// One step of the differential below.
#[derive(Clone, Debug)]
enum Step {
    AltOn,
    AltOff,
    Print,
    Run,
    Finish,
    UserWrote { first_input: bool, ctrl_c: bool },
    Typed,
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    vec(
        prop_oneof![
            Just(Step::AltOn),
            Just(Step::AltOff),
            Just(Step::Print),
            Just(Step::Run),
            Just(Step::Finish),
            (any::<bool>(), any::<bool>()).prop_map(|(first_input, ctrl_c)| Step::UserWrote {
                first_input,
                ctrl_c
            }),
            Just(Step::Typed),
        ],
        0..40,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The facts the key path reads without the model's lock equal the facts it would read under
    /// it, after every parse slice, whatever the order of screen switches, commands and the key
    /// path's own bookkeeping (the differential of the lock-free read against the locked one).
    #[test]
    fn published_facts_equal_the_locked_read_after_every_slice(steps in steps()) {
        let mut model = TerminalModel::mock(None, None);
        let cell = model.key_facts_cell();
        let inbox = model.model_inbox();
        model.publish_key_facts();
        let mut running = false;
        for step in steps {
            match step {
                Step::AltOn => parse_slice_like_the_event_loop(&mut model, "\x1b[?1049h"),
                Step::AltOff => parse_slice_like_the_event_loop(&mut model, "\x1b[?1049l"),
                Step::Print => parse_slice_like_the_event_loop(&mut model, "output\r\n"),
                Step::Run if !running => {
                    model.block_list_mut().active_block_mut().start();
                    model.simulate_cmd("tool");
                    model
                        .block_list_mut()
                        .active_block_mut()
                        .set_was_long_running(true.into());
                    running = true;
                    model.publish_key_facts();
                }
                Step::Finish if running => {
                    model.finish_block();
                    running = false;
                    model.publish_key_facts();
                }
                Step::UserWrote { first_input, ctrl_c } => {
                    inbox.post(ModelCommand::UserWrote { first_input, ctrl_c });
                    parse_slice_like_the_event_loop(&mut model, "");
                }
                Step::Typed => {
                    inbox.post(ModelCommand::TypedAhead(b"x".to_vec()));
                    parse_slice_like_the_event_loop(&mut model, "x");
                }
                Step::Run | Step::Finish => {}
            }
            assert_eq!(cell.load(), KeyFacts::from_model(&model));
        }
    }
}

// ---- the key snapshot (#1404) -------------------------------------------------------------

#[test]
fn a_parse_publishes_the_snapshot_the_key_path_reads_without_the_lock() {
    let mut model = TerminalModel::mock(None, None);
    let cell = model.key_model_cell();
    model.publish_key_facts();
    let idle = cell.load();
    assert!(!idle.read_only);
    assert!(idle.pending_session_id.is_none());
    assert!(!idle.has_pending_ssh_session && !idle.has_pending_subshell);
    assert!(!idle.agent_owns_pty, "no agent drives the command");
    assert!(
        idle.interactive_latch.is_some(),
        "the key probe finds the active block's latch"
    );
    assert_eq!(
        idle.active_block_hash,
        block_id_hash(model.active_block_id()),
        "the block the view noted is told from the active one by hash"
    );
    // A parse republishes: the alt screen is part of the same publication.
    parse_slice_like_the_event_loop(&mut model, "\x1b[?1049h");
    assert!(cell.load().interactive_latch.is_some());
}

#[test]
fn the_snapshot_cell_returns_what_was_stored() {
    let cell = KeyModelCell::default();
    assert_eq!(cell.load().active_block_hash, 0);
    cell.store(KeyModelSnapshot {
        read_only: true,
        active_block_started: true,
        active_block_hash: 7,
        ..KeyModelSnapshot::default()
    });
    let loaded = cell.load();
    assert!(loaded.read_only && loaded.active_block_started);
    assert_eq!(loaded.active_block_hash, 7);
    assert!(
        !loaded.is_long_running(),
        "a read-only session is never long running"
    );
}

#[test]
fn user_input_left_in_the_inbox_is_applied_in_order_with_the_rest() {
    let mut model = TerminalModel::mock(None, None);
    let inbox = model.model_inbox();
    inbox.post(ModelCommand::UserInput("ls".to_owned()));
    inbox.post(ModelCommand::TypedAhead(b"x".to_vec()));
    assert!(inbox.has_pending());
    model.apply_inbox();
    assert!(
        !inbox.has_pending(),
        "applied, in order, before the next parse"
    );
}

/// #1442: an inbox nobody drains stops at its cap; data commands past it are dropped and counted
/// (the view tells the user), the kept ones stay in posting order.
#[test]
fn a_full_inbox_drops_data_commands_and_reports_them_once() {
    let inbox = ModelInbox::with_capacity(3);
    for n in 0u8..3 {
        assert!(inbox.post(ModelCommand::TypedAhead(vec![n])));
    }
    assert!(!inbox.post(ModelCommand::TypedAhead(vec![9])));
    assert!(!inbox.post(ModelCommand::UserInput("x".to_owned())));
    assert!(!inbox.post(ModelCommand::SharedSessionWrite(vec![1])));
    assert_eq!(inbox.take_overflow_report(), 3);
    assert_eq!(inbox.take_overflow_report(), 0, "reported once");
    assert_eq!(
        inbox.take(),
        vec![
            ModelCommand::TypedAhead(vec![0]),
            ModelCommand::TypedAhead(vec![1]),
            ModelCommand::TypedAhead(vec![2]),
        ]
    );
    // Drained: room again.
    assert!(inbox.post(ModelCommand::TypedAhead(vec![7])));
}

/// A ^C and the first input are flags, never dropped: past the cap they merge into the newest
/// `UserWrote`.
#[test]
fn a_full_inbox_never_loses_a_ctrl_c_or_the_first_input() {
    let inbox = ModelInbox::with_capacity(2);
    assert!(inbox.post(ModelCommand::TypedAhead(vec![1])));
    assert!(inbox.post(ModelCommand::TypedAhead(vec![2])));
    assert!(inbox.post(ModelCommand::UserWrote {
        first_input: true,
        ctrl_c: false,
    }));
    assert!(inbox.post(ModelCommand::UserWrote {
        first_input: false,
        ctrl_c: true,
    }));
    let taken = inbox.take();
    assert_eq!(taken.len(), 3, "the second UserWrote merged into the first");
    assert_eq!(
        taken[2],
        ModelCommand::UserWrote {
            first_input: true,
            ctrl_c: true,
        }
    );
    assert_eq!(inbox.take_overflow_report(), 0, "no data was dropped");
}

#[test]
fn the_default_inbox_cap_is_far_above_a_long_gate_at_key_repeat_rate() {
    assert!(MAX_INBOX_COMMANDS >= 180 * 60 * 3 * 4);
}

// ---- the publication gen (#1424) ----------------------------------------------------------

#[test]
fn the_gen_is_stored_with_the_bits_and_a_plain_store_keeps_it() {
    let cell = KeyFactsCell::default();
    assert_eq!(cell.load_with_gen(), (KeyFacts::default(), 0));
    let facts = KeyFacts {
        alt_screen: true,
        ..KeyFacts::default()
    };
    cell.store_with_gen(facts, 41);
    assert_eq!(cell.load_with_gen(), (facts, 41));
    // A main-thread refresh stores what it read from the model without claiming a publication.
    let refreshed = KeyFacts {
        agent_in_control: true,
        ..facts
    };
    cell.store(refreshed);
    assert_eq!(cell.load_with_gen(), (refreshed, 41));
    // The first-input bit is a flag, not a publication either.
    cell.note_received_user_input();
    let (loaded, generation) = cell.load_with_gen();
    assert!(loaded.received_user_input);
    assert_eq!(generation, 41);
}

#[test]
fn every_combination_of_facts_survives_every_gen() {
    let cell = KeyFactsCell::default();
    for (generation, facts) in every_facts().enumerate() {
        cell.store_with_gen(facts, generation as u64 + 1);
        assert_eq!(cell.load_with_gen(), (facts, generation as u32 + 1));
    }
}

fn snapshot_at(generation: u64) -> KeyModelSnapshot {
    KeyModelSnapshot {
        facts_gen: generation,
        active_block_hash: generation,
        ..KeyModelSnapshot::default()
    }
}

#[test]
fn a_reader_takes_the_pair_only_when_both_cells_carry_one_gen() {
    let facts = KeyFactsCell::default();
    let model = KeyModelCell::default();
    // Never published: gen 0 in both.
    let read = load_consistent(&facts, &model);
    assert!(read.consistent);
    assert_eq!(read.facts_gen, 0);

    model.store(snapshot_at(5));
    facts.store_with_gen(KeyFacts::default(), 5);
    let read = load_consistent(&facts, &model);
    assert!(read.consistent);
    assert_eq!(read.facts_gen, 5);
    assert_eq!(read.snapshot.active_block_hash, 5);
    assert_eq!(load_gen_consistent(&facts, &model), (5, true));
}

#[test]
fn a_pair_caught_between_the_two_stores_is_reported_torn_with_the_older_gen() {
    let facts = KeyFactsCell::default();
    let model = KeyModelCell::default();
    // The publisher stores the snapshot first; the facts still carry the previous gen.
    facts.store_with_gen(KeyFacts::default(), 7);
    model.store(snapshot_at(8));
    let read = load_consistent(&facts, &model);
    assert!(
        !read.consistent,
        "no retry can fix a publisher that stopped midway"
    );
    assert_eq!(read.facts_gen, 7, "a stamp from it is early, never late");
    assert_eq!(load_gen_consistent(&facts, &model), (7, false));
    // The facts store completes: the next read agrees.
    facts.store_with_gen(KeyFacts::default(), 8);
    assert_eq!(load_gen_consistent(&facts, &model), (8, true));
}

#[test]
fn every_publication_bumps_the_gen_in_both_cells() {
    let model = TerminalModel::mock(None, None);
    let facts_cell = model.key_facts_cell();
    let model_cell = model.key_model_cell();
    let before = model.facts_gen();
    model.publish_key_facts();
    model.publish_key_facts();
    assert_eq!(model.facts_gen(), before + 2);
    let read = load_consistent(&facts_cell, &model_cell);
    assert!(read.consistent);
    assert_eq!(read.facts_gen, before + 2);
    assert_eq!(read.snapshot.facts_gen, before + 2);
    // A snapshot built for a refresh carries the gen of the last publication, unchanged.
    let refreshed = KeyModelSnapshot::from_model(&model);
    assert_eq!(refreshed.facts_gen, before + 2);
    assert_eq!(model.facts_gen(), before + 2, "reading never publishes");
}

#[test]
fn the_snapshot_carries_the_paste_route_bits_the_view_used_to_lock_for() {
    let mut model = TerminalModel::mock(None, None);
    let snapshot = KeyModelSnapshot::from_model(&model);
    assert_eq!(
        snapshot.is_bootstrapped,
        model.block_list().is_bootstrapped()
    );
    assert_eq!(
        snapshot.needs_bracketed_paste,
        model.needs_bracketed_paste()
    );
}

// ---- the sticky output bits ---------------------------------------------------------------

#[test]
fn visible_text_is_sticky_for_the_block_and_scanned_only_until_seen() {
    let mut sticky = StickyOutput::default();
    let mut scans = 0;
    // Nothing visible yet: scanned, not seen, and no unterminated scan (it needs visible text).
    let seen = sticky.observe(
        1,
        10,
        || {
            scans += 1;
            false
        },
        || panic!("an unterminated scan without visible text"),
    );
    assert_eq!(seen, (false, None));
    // Text appears.
    let seen = sticky.observe(1, 11, || true, || false);
    assert_eq!(seen, (true, None));
    // Seen: the visible scan never runs again, even if the output would now scan empty.
    let seen = sticky.observe(1, 12, || panic!("rescanned a sticky bit"), || false);
    assert_eq!(seen, (true, None));
    assert_eq!(scans, 1);
    // A new block starts over.
    let seen = sticky.observe(2, 13, || false, || panic!("no text in the new block"));
    assert_eq!(seen, (false, None));
}

#[test]
fn unterminated_since_is_the_gen_it_became_true_and_clears_when_it_stops() {
    let mut sticky = StickyOutput::default();
    // Visible text, cursor mid-line from gen 20.
    assert_eq!(sticky.observe(1, 19, || true, || false), (true, None));
    assert_eq!(sticky.observe(1, 20, || true, || true), (true, Some(20)));
    // Still unterminated at later gens: the gen it BECAME true stays.
    assert_eq!(sticky.observe(1, 21, || true, || true), (true, Some(20)));
    assert_eq!(sticky.observe(1, 25, || true, || true), (true, Some(20)));
    // The line ends: cleared; the next unterminated line is a new prompt with a new gen.
    assert_eq!(sticky.observe(1, 26, || true, || false), (true, None));
    assert_eq!(sticky.observe(1, 30, || true, || true), (true, Some(30)));
    // A new block forgets it.
    assert_eq!(sticky.observe(2, 31, || true, || false), (true, None));
}
