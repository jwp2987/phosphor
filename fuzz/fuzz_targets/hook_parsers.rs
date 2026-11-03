//! The shell-hook parsers behind the ansi processor: DCS hooks (`ESC P $ d <hex>` and `$ f`),
//! the hook OSCs 9278 (`f` json, `d` hex) and 9277, and the setup-line marker OSC 7771.
//!
//! The first byte picks how the rest is framed, so the fuzzer mutates payloads inside valid
//! frames instead of rediscovering the escape syntax: 0 raw, 1 DCS `$d` hex, 2 DCS `$f`, 3 OSC
//! 9278 `f`, 4 OSC 9278 `d` hex, 5 OSC 7771, 6 OSC 9277.
#![no_main]

use libfuzzer_sys::fuzz_target;
use warp::terminal::model::terminal_model::TerminalModel;

fn hex(bytes: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[usize::from(byte >> 4)],
                DIGITS[usize::from(byte & 15)],
            ]
        })
        .collect()
}

fn frame(selector: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    match selector % 7 {
        0 => out.extend_from_slice(payload),
        1 => {
            out.extend_from_slice(b"\x1bP$d");
            out.extend(hex(payload));
            out.extend_from_slice(b"\x1b\\");
        }
        2 => {
            out.extend_from_slice(b"\x1bP$f");
            out.extend_from_slice(payload);
            out.extend_from_slice(b"\x1b\\");
        }
        3 => {
            out.extend_from_slice(b"\x1b]9278;f;");
            out.extend_from_slice(payload);
            out.push(0x07);
        }
        4 => {
            out.extend_from_slice(b"\x1b]9278;d;");
            out.extend(hex(payload));
            out.push(0x07);
        }
        5 => {
            out.extend_from_slice(b"\x1b]7771;");
            out.extend_from_slice(payload);
            out.push(0x07);
        }
        _ => {
            out.extend_from_slice(b"\x1b]9277;");
            out.extend_from_slice(payload);
            out.push(0x07);
        }
    }
    out
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, payload)) = data.split_first() else {
        return;
    };
    let mut model = TerminalModel::mock(None, None);
    model.process_bytes(frame(selector, payload).as_slice());
});
