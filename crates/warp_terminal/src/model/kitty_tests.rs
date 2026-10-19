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
        let parsed = parse_kitty_chunk(chunk);
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

    // At the dimension cap (with the other side small enough to stay under the byte cap) the
    // dimensions are fine; the (empty) payload is what's wrong.
    let result = kitty_action_from_chunks(format!("a=t,f=32,s={max},v=1").as_bytes());
    assert!(is_size_mismatch(&result), "{result:?}");

    // A square at the cap would be 400 MB of RGBA, over the byte cap (#880, which is kept
    // under the image cache's 320 MB budget), so it is rejected up front too.
    let result = kitty_action_from_chunks(format!("a=t,f=32,s={max},v={max}").as_bytes());
    assert!(is_dimensions_too_large(&result), "{result:?}");

    // One past the cap in either dimension is rejected up front.
    for control in [
        format!("a=t,f=32,s={},v=1", max + 1),
        format!("a=t,f=32,s=1,v={}", max + 1),
    ] {
        let result = kitty_action_from_chunks(control.as_bytes());
        assert!(is_dimensions_too_large(&result), "{control}: {result:?}");
    }

    // The byte cap is exactly an 8192x8192 RGBA square, and it stays under the
    // asset cache's 320 MB budget.
    assert!(MAX_KITTY_IMAGE_BYTES < 320 * 1000 * 1000);
    let side = 8192;
    let mut control_data = KittyControlData {
        width: side,
        height: side,
        ..Default::default()
    };
    assert_eq!(
        raw_pixel_byte_count(&control_data).unwrap(),
        Some(MAX_KITTY_IMAGE_BYTES)
    );
    control_data.height = side + 1;
    assert!(raw_pixel_byte_count(&control_data).is_err());
    control_data.height = side;
    control_data.pixel_data_format = KittyPixelDataFormat::Rgb24Bit;
    assert_eq!(
        raw_pixel_byte_count(&control_data).unwrap(),
        Some(3 * side as usize * side as usize)
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

fn placement(cols: Option<u32>, rows: Option<u32>) -> KittyPlacementData {
    KittyPlacementData {
        cols,
        rows,
        ..Default::default()
    }
}

fn assert_size(size: Vector2F, width: f32, height: f32) {
    assert!(
        (size.x() - width).abs() < 0.5 && (size.y() - height).abs() < 0.5,
        "expected {width}x{height}, got {size:?}"
    );
}

/// #876: `c=`/`r=` are untrusted u32s. The caller turns the desired height
/// into one newline per cell, so an unbounded height stalled the model thread
/// (`r=4294967295` -> ~2.7e8 newlines). The result must stay inside
/// `max_width_px` x `max_height_px` however large `c`/`r` are.
#[test]
fn desired_dimensions_are_bounded() {
    let image = Vector2F::new(10.0, 10.0);
    // cell_height 20, cell_width 10, bounds 800x600.
    let desired = |p: KittyPlacementData| p.get_desired_dimensions(image, 20, 10, 800, 600);

    // Both given: each dimension is clamped independently.
    assert_size(
        desired(placement(Some(u32::MAX), Some(u32::MAX))),
        800.0,
        600.0,
    );
    // One given: the aspect ratio is kept while fitting the bounds.
    assert_size(desired(placement(Some(u32::MAX), None)), 600.0, 600.0);
    assert_size(desired(placement(None, Some(u32::MAX))), 600.0, 600.0);

    // Huge but not overflowing (16 * 268435455 fits in u32).
    let size = placement(None, Some(268_435_455)).get_desired_dimensions(image, 16, 8, 800, 600);
    assert_size(size, 600.0, 600.0);
    let size = placement(Some(1), Some(268_435_455)).get_desired_dimensions(image, 16, 8, 800, 600);
    assert_size(size, 8.0, 600.0);

    // In-bounds requests are unchanged.
    assert_size(desired(placement(Some(4), Some(3))), 40.0, 60.0);
    let wide = Vector2F::new(20.0, 10.0);
    assert_size(
        placement(Some(4), None).get_desired_dimensions(wide, 20, 10, 800, 600),
        40.0,
        20.0,
    );
    assert_size(desired(placement(None, None)), 10.0, 10.0);
}

#[test]
fn decompression_is_bounded_by_the_declared_image_size() {
    let mut encoder = ZlibEncoder::new(vec![], Compression::best());
    encoder.write_all(&[0; 4096]).unwrap();
    let compressed = encoder.finish().unwrap();

    assert!(matches!(
        decode_kitty_image_data_bounded(compressed.clone(), true, 1024),
        Err(KittyDecodeError::PayloadTooLarge { limit: 1024 })
    ));
    assert_eq!(
        decode_kitty_image_data_bounded(compressed.clone(), true, 4096).unwrap(),
        vec![0; 4096]
    );

    // End to end: a 2x2 RGBA image (16 bytes) whose zlib stream inflates to
    // 4096 bytes stops inflating at 16 rather than buffering it all.
    let input = format!("a=t,f=32,o=z,s=2,v=2;{}", b64(&compressed));
    let result = kitty_action_from_chunks(input.as_bytes());
    assert!(
        matches!(
            result,
            Err(KittyError::InvalidKittyAction(
                InvalidKittyAction::InvalidKittyPayload(InvalidKittyPayload::KittyDecodeError(
                    KittyDecodeError::PayloadTooLarge { limit: 16 }
                ))
            ))
        ),
        "{result:?}"
    );

    // A tiny image whose zlib stream is larger than its pixels still decodes.
    let mut encoder = ZlibEncoder::new(vec![], Compression::default());
    encoder.write_all(&[7, 8, 9]).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() > 3);
    let input = format!("a=t,f=24,o=z,s=1,v=1;{}", b64(&compressed));
    let result = kitty_action_from_chunks(input.as_bytes());
    assert!(
        matches!(result, Ok(KittyAction::StoreOnly(_))),
        "{result:?}"
    );
}

#[test]
fn assembled_payload_is_bounded() {
    let chunks = vec![b64(&[1; 6]).into_bytes(), b64(&[2; 6]).into_bytes()];
    assert_eq!(decode_base64_chunks(&chunks, 12).unwrap().len(), 12);
    assert!(matches!(
        decode_base64_chunks(&chunks, 11),
        Err(KittyDecodeError::PayloadTooLarge { limit: 11 })
    ));
    // The first chunk alone fits; the check is on the running total.
    assert!(matches!(
        decode_base64_chunks(&chunks, 6),
        Err(KittyDecodeError::PayloadTooLarge { limit: 6 })
    ));
}

/// #880: f=100 is PNG only; a GIF (or any other decoder-reachable format) must
/// be rejected rather than handed to a decoder.
#[test]
fn f100_rejects_non_png_payloads() {
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0]);
    let payload = base64::engine::general_purpose::STANDARD.encode(&gif);
    let control = format!("a=t,f=100;{payload}");
    let result = kitty_action_from_chunks(control.as_bytes());
    assert!(result.is_err(), "{result:?}");
}

#[cfg(feature = "local_fs")]
mod file_read {
    use super::*;
    use std::time::Duration;

    /// #824: a stalled read must not hold the caller past the deadline.
    #[test]
    fn run_with_timeout_gives_up_on_a_stalled_worker() {
        let started = std::time::Instant::now();
        let result = run_with_timeout(Duration::from_millis(20), || {
            std::thread::sleep(Duration::from_secs(5));
            1
        });
        assert!(result.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn run_with_timeout_returns_the_result_of_fast_work() {
        assert_eq!(run_with_timeout(Duration::from_secs(5), || 7), Some(7));
    }

    /// #824: the in-flight cap refuses past its limit and frees slots on release.
    #[test]
    fn slot_cap_refuses_past_the_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = AtomicUsize::new(0);
        for _ in 0..3 {
            assert!(try_acquire_slot(&counter, 3));
        }
        assert!(!try_acquire_slot(&counter, 3));
        counter.fetch_sub(1, Ordering::AcqRel);
        assert!(try_acquire_slot(&counter, 3));
        assert_eq!(counter.load(Ordering::Acquire), 3);
    }

    /// #824: a helper that finishes releases its slot.
    #[test]
    fn finished_helper_releases_its_slot() {
        let before = IN_FLIGHT_FILE_READS.load(std::sync::atomic::Ordering::Acquire);
        assert_eq!(run_with_timeout(Duration::from_secs(5), || 1), Some(1));
        // The guard drops right after the send; give the thread a moment.
        for _ in 0..100 {
            if IN_FLIGHT_FILE_READS.load(std::sync::atomic::Ordering::Acquire) <= before {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("in-flight slot was never released");
    }

    /// #824: an abandoned `t=t` read must leave the temp file alone.
    #[test]
    fn abandoned_temp_read_does_not_delete_the_file() {
        use std::sync::atomic::AtomicBool;
        let dir = std::env::temp_dir();
        let path = dir.join("kitty-abandon-test-tty-graphics-protocol.png");
        std::fs::write(&path, b"x").unwrap();
        let abandoned = AtomicBool::new(true);
        let _ = read_file_blocking(path.to_str().unwrap().as_bytes().to_vec(), true, &abandoned);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    /// A regular file named by `t=f` is still read (now on a helper thread).
    #[test]
    fn simple_file_is_read_off_thread() {
        let dir = env::temp_dir().join(format!("kitty-read-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("image.bin");
        fs::write(&path, b"pixels").unwrap();

        let data = read_file(path.to_str().unwrap().as_bytes().to_vec(), false);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(data.unwrap(), b"pixels");
    }

    /// A nonexistent file is an error reply, not a hang or a panic.
    #[test]
    fn missing_file_is_an_error() {
        let result = read_file(b"/nonexistent/kitty-read-test".to_vec(), false);
        assert!(matches!(
            result,
            Err(InvalidKittyPayload::FileError(FileError::FileReadError(_)))
        ));
    }
}
