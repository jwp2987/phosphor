//! The ANSI / VT parser (`terminal/model/ansi`) fed arbitrary pty output. Nothing may panic or
//! hang. The first byte picks the read size (1..=64 bytes) so parser resumption across read
//! boundaries is exercised, as the pty reader's 4 KiB reads do in practice.
#![no_main]

use libfuzzer_sys::fuzz_target;
use warp::terminal::model::terminal_model::TerminalModel;

fuzz_target!(|data: &[u8]| {
    let Some((&size, bytes)) = data.split_first() else {
        return;
    };
    let read = usize::from(size % 64) + 1;
    let mut model = TerminalModel::mock(None, None);
    for chunk in bytes.chunks(read) {
        model.process_bytes(chunk);
    }
});
