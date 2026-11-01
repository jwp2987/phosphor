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
