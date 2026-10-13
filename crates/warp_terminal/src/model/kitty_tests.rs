use std::io::Write;

use base64::Engine;
use flate2::{Compression, write::ZlibEncoder};

use super::*;

/// Assembles one kitty transmission from `0x1e`-separated chunks exactly the
/// way the `image_protocols` fuzz target (and the terminal model) does: the
/// first chunk's control data, every chunk's payload.
fn kitty_action_from_chunks(input: &[u8]) -> Result<KittyAction, KittyError> {
    let mut control_data = None;
    let mut payload = vec![];
    for chunk in input.split(|&byte| byte == 0x1e) {
        let parsed = parse_kitty_chunk(chunk.to_vec());
        if control_data.is_none() {
            control_data = Some(parsed.control_data.clone());
        }
        payload.push(parsed.payload);
    }
    let pending = PendingKittyMessage {
        control_data: control_data.expect("split always yields one chunk"),
        payload,
    };
    let message = KittyMessage::try_from(pending).map_err(KittyError::from)?;
    KittyAction::try_from(message)
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn is_dimensions_too_large(result: &Result<KittyAction, KittyError>) -> bool {
    matches!(
        result,
        Err(KittyError::InvalidKittyAction(
            InvalidKittyAction::InvalidKittyPayload(InvalidKittyPayload::InvalidKittyImage(
                InvalidKittyImage::KittyRgbError(KittyRgbError::DimensionsTooLarge { .. })
            ))
        ))
    )
}

fn is_size_mismatch(result: &Result<KittyAction, KittyError>) -> bool {
    matches!(
        result,
        Err(KittyError::InvalidKittyAction(
            InvalidKittyAction::InvalidKittyPayload(InvalidKittyPayload::InvalidKittyImage(
                InvalidKittyImage::KittyRgbError(KittyRgbError::ExpectedDataSizeMismatch { .. })
            ))
        ))
    )
}

/// #876: the exact cargo-fuzz crash input (`\x1es=2555555558`; byte 0 is the
/// fuzz target's parser selector) panicked with `attempt to multiply with
/// overflow` in `CustomImageHeader::prepend_custom_header`.
#[test]
fn fuzz_crash_876_is_an_error_not_a_panic() {
    let crash: &[u8] = b"\x1es=2555555558";
    let (&selector, rest) = crash.split_first().unwrap();
    assert_eq!(selector & 1, 0, "selector routes to the kitty parser");

    let result = kitty_action_from_chunks(rest);
    assert!(is_dimensions_too_large(&result), "{result:?}");
}

#[test]
fn u32_max_dimensions_are_rejected() {
    for control in [
        "a=t,f=32,s=4294967295,v=1",
        "a=t,f=32,s=1,v=4294967295",
        "a=t,f=24,s=4294967295,v=4294967295",
        "a=T,f=32,s=4294967295,v=4294967295",
        "a=q,f=24,s=4294967295,v=4294967295",
    ] {
        let result = kitty_action_from_chunks(control.as_bytes());
        assert!(is_dimensions_too_large(&result), "{control}: {result:?}");
    }
}

/// 4 * 65536 * 65536 wraps to exactly 0 in u32: before #876 an empty payload
/// was accepted (in builds without overflow checks) as a 65536x65536 image.
#[test]
fn size_that_wraps_to_zero_is_not_accepted() {
    let result = kitty_action_from_chunks(b"a=t,f=32,s=65536,v=65536;");
    assert!(is_dimensions_too_large(&result), "{result:?}");
}

#[test]
fn dimension_cap_boundary() {
    let max = MAX_KITTY_IMAGE_DIMENSION;

    // At the cap the dimensions are fine; the (empty) payload is what's wrong.
    let result = kitty_action_from_chunks(format!("a=t,f=32,s={max},v={max}").as_bytes());
    assert!(is_size_mismatch(&result), "{result:?}");

    // One past the cap in either dimension is rejected up front.
    for control in [
        format!("a=t,f=32,s={},v=1", max + 1),
        format!("a=t,f=32,s=1,v={}", max + 1),
    ] {
        let result = kitty_action_from_chunks(control.as_bytes());
        assert!(is_dimensions_too_large(&result), "{control}: {result:?}");
    }

    // The byte cap is exactly a cap-sized RGBA square.
    let mut control_data = KittyControlData {
        width: max,
        height: max,
        ..Default::default()
    };
    assert_eq!(
        raw_pixel_byte_count(&control_data).unwrap(),
        Some(MAX_KITTY_IMAGE_BYTES)
    );
    control_data.pixel_data_format = KittyPixelDataFormat::Rgb24Bit;
    assert_eq!(
        raw_pixel_byte_count(&control_data).unwrap(),
        Some(3 * max as usize * max as usize)
    );
    control_data.pixel_data_format = KittyPixelDataFormat::Png;
    control_data.width = u32::MAX;
    assert_eq!(raw_pixel_byte_count(&control_data).unwrap(), None);
}

/// PNG takes its size from the image, so `s=`/`v=` do not matter for it.
#[test]
fn png_ignores_declared_dimensions() {
    let result = kitty_action_from_chunks(b"a=t,f=100,s=4294967295,v=4294967295;AAAA");
    assert!(!is_dimensions_too_large(&result), "{result:?}");
    assert!(result.is_err(), "garbage is not a PNG");
}

#[test]
fn valid_rgb_image_still_decodes() {
    let pixels = [1u8, 2, 3, 4, 5, 6];
    let input = format!("a=t,f=24,s=2,v=1,i=7;{}", b64(&pixels));
    let Ok(KittyAction::StoreOnly(action)) = kitty_action_from_chunks(input.as_bytes()) else {
        panic!("a 2x1 RGB image must decode");
    };
    assert_eq!(action.image_id, 7);
    assert_eq!(
        action.image.data,
        b"warp-img:rgb:2:1:\x01\x02\x03\x04\x05\x06"
    );
}

#[test]
fn valid_chunked_compressed_rgba_image_still_decodes() {
    let pixels = [9u8; 2 * 2 * 4];
    let mut encoder = ZlibEncoder::new(vec![], Compression::default());
    encoder.write_all(&pixels).unwrap();
    let encoded = b64(&encoder.finish().unwrap());
    let (first, second) = encoded.split_at(encoded.len() / 2 / 4 * 4);
    let input = format!("a=t,f=32,o=z,s=2,v=2,m=1;{first}\x1em=0;{second}");
    let result = kitty_action_from_chunks(input.as_bytes());
    let Ok(KittyAction::StoreOnly(action)) = result else {
        panic!("a chunked, compressed 2x2 RGBA image must decode: {result:?}");
    };
    assert!(action.image.data.starts_with(b"warp-img:rgba:2:2:"));
    assert!(action.image.data.ends_with(&pixels));
}

#[test]
fn read_at_most_stops_at_the_limit() {
    let endless = std::io::repeat(0);
    assert_eq!(read_at_most(Ok(endless), 16).unwrap(), None);

    let exact: &[u8] = &[1; 16];
    assert_eq!(read_at_most(Ok(exact), 16).unwrap(), Some(vec![1; 16]));

    let short: &[u8] = &[1; 3];
    assert_eq!(read_at_most(Ok(short), 16).unwrap(), Some(vec![1; 3]));
}

#[test]
fn decompression_is_bounded() {
    // A compressed stream that inflates past the limit is refused rather than
    // buffered whole.
    let mut encoder = ZlibEncoder::new(vec![], Compression::best());
    encoder.write_all(&[0; 4096]).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < 100);
    assert_eq!(
        read_at_most(Ok(flate2::read::ZlibDecoder::new(&compressed[..])), 1024).unwrap(),
        None
    );
}

#[test]
fn desired_dimensions_do_not_overflow() {
    let placement = KittyPlacementData {
        cols: Some(u32::MAX),
        rows: Some(u32::MAX),
        ..Default::default()
    };
    let size = placement.get_desired_dimensions(Vector2F::new(10.0, 10.0), 20, 10, 800, 600);
    assert!(size.x().is_finite() && size.y().is_finite());

    let placement = KittyPlacementData {
        cols: Some(u32::MAX),
        ..Default::default()
    };
    let size = placement.get_desired_dimensions(Vector2F::new(10.0, 10.0), 20, 10, 800, 600);
    assert!(size.x().is_finite() && size.y().is_finite());
}
