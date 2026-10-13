//! libFuzzer target: one input = a decoded sequence of pty reads and model
//! operations on a fresh `TerminalModel` (see `fuzz_api::run_fuzz_input` for
//! the input layout). Panics -- including `debug_assert!`/`safe_assert!` when
//! built with `cargo fuzz run -a` -- and invariant violations are crashes.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    warp::terminal::model::fuzz_api::run_fuzz_input(data);
});
