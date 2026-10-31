use super::*;

#[test]
fn frame_index_advances_every_interval_and_wraps() {
    assert_eq!(frame_index_for_elapsed_ms(0), 0);
    assert_eq!(frame_index_for_elapsed_ms(FRAME_INTERVAL_MS - 1), 0);
    assert_eq!(frame_index_for_elapsed_ms(FRAME_INTERVAL_MS), 1);
    assert_eq!(
        frame_index_for_elapsed_ms(FRAME_INTERVAL_MS * FRAMES.len() as u64),
        0
    );
    assert_eq!(
        frame_index_for_elapsed_ms(FRAME_INTERVAL_MS * (FRAMES.len() as u64 + 3)),
        3
    );
}
