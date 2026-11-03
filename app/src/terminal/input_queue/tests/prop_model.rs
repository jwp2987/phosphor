//! The model-based invariant test with `proptest` strategies (owner request, #1353).
//!
//! The seeded `run_seed` test in the parent module stays (it needs no dependency). This one
//! generates a `Vec<Op>` and lets proptest SHRINK a failure to the shortest interleaving that
//! breaks an invariant. Entries come from both origins of the pane queue (editor keys and editor
//! pastes, pty bytes and pty pastes, destination-neutral text and Enter), the gates are every
//! kind the queue knows, and the ops include gate events, live and stale give-ups, focus flips,
//! signals, facts changes and requeues (the later stage turning a delivery into a hold).
//!
//! Checked after every step against the reference [`Model`] (written from the spec, not from the
//! queue) plus invariants that do not depend on it, all in `check_equal`: nothing lost, nothing
//! duplicated, no overtaking, a non-empty queue has a blocked head. At the end: a cap fires only
//! for a gate whose event had not come (once), and every remaining gate opens by its event.

use proptest::collection::vec;
use proptest::prelude::{Just, ProptestConfig, Strategy, any, prop_oneof, proptest};

use super::ops::{Op, run_ops};

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        8 => any::<u8>().prop_map(Op::Text),
        4 => Just(Op::Enter),
        2 => Just(Op::Control),
        2 => Just(Op::Seq),
        1 => Just(Op::Raw),
        6 => any::<u8>().prop_map(Op::EditorKey),
        6 => any::<u8>().prop_map(Op::PtyKey),
        4 => any::<bool>().prop_map(|editor| Op::Paste { editor }),
        4 => (any::<u8>(), any::<bool>()).prop_map(|(pick, ok)| Op::PasteResult { pick, ok }),
        8 => (any::<u8>(), proptest::option::of((any::<u8>(), any::<u8>())))
            .prop_map(|(pick, made_up)| Op::GateEvent { pick, made_up }),
        5 => (any::<u8>(), any::<u8>()).prop_map(|(kind, key)| Op::OpenGate { kind, key }),
        4 => any::<u8>().prop_map(|pick| Op::GiveUpLive { pick }),
        3 => any::<u8>().prop_map(|pick| Op::GiveUpStale { pick }),
        6 => any::<u8>().prop_map(Op::Signal),
        2 => Just(Op::Focus),
        4 => any::<u8>().prop_map(Op::Facts),
        1 => Just(Op::EnterOpens),
        1 => Just(Op::SshWait),
        3 => (any::<u8>(), any::<u8>()).prop_map(|(kind, key)| Op::RequeueNext { kind, key }),
        3 => (any::<u8>(), any::<u8>()).prop_map(|(class, origin)| Op::Decide { class, origin }),
        3 => any::<u8>().prop_map(|kind| Op::DecideEquiv { kind }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn multi_origin_interleavings_hold_the_invariants(ops in vec(op_strategy(), 1..250)) {
        run_ops(&ops);
    }
}
