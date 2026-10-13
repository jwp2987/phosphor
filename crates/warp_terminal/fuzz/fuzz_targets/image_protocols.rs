//! libFuzzer target for the image-protocol payload parsers that run on raw pty
//! bytes: kitty graphics APC chunks (control data, base64, zlib, RGB/RGBA/PNG
//! sizing) and iTerm OSC 1337 image metadata.
//!
//! Byte 0 selects the parser. For kitty, the rest is split on 0x1e into the
//! chunks of one transmission, assembled the way the terminal model assembles
//! them (first chunk's control data, every chunk's payload) and converted to a
//! `KittyAction`. For iTerm, the rest is split on `;` into OSC parameters.
#![no_main]

use libfuzzer_sys::fuzz_target;
use warp_terminal::model::iterm_image::parse_iterm_image_metadata;
use warp_terminal::model::kitty::{
    KittyAction, KittyMessage, PendingKittyMessage, parse_kitty_chunk,
};

fuzz_target!(|data: &[u8]| {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    if selector & 1 == 0 {
        let mut control_data = None;
        let mut payload = vec![];
        for chunk in rest.split(|&byte| byte == 0x1e) {
            let parsed = parse_kitty_chunk(chunk.to_vec());
            if control_data.is_none() {
                control_data = Some(parsed.control_data.clone());
            }
            payload.push(parsed.payload);
        }
        let Some(control_data) = control_data else {
            return;
        };
        let pending = PendingKittyMessage {
            control_data,
            payload,
        };
        if let Ok(message) = KittyMessage::try_from(pending) {
            let _ = KittyAction::try_from(message);
        }
    } else {
        let params: Vec<&[u8]> = rest.split(|&byte| byte == b';').collect();
        let _ = parse_iterm_image_metadata(&params);
    }
});
