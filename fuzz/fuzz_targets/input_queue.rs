//! The pane input queue under arbitrary operation sequences: the same ops and the same checks as
//! the proptest model (`app/src/terminal/input_queue/tests/prop_model.rs`), run by
//! `warp::dev_support::run_ops`, which compares every step against the reference model and
//! asserts the queue's invariants (nothing lost, duplicated or overtaking; a blocked head
//! blocks; a give-up fires once, only for a gate whose event had not come; never wedged).
#![no_main]

use libfuzzer_sys::fuzz_target;
use warp::dev_support::{Op, run_ops};

fuzz_target!(|ops: Vec<Op>| {
    // The proptest model runs sequences of up to 250 steps; a longer one adds time, not shape.
    if !ops.is_empty() && ops.len() <= 250 {
        run_ops(&ops);
    }
});
