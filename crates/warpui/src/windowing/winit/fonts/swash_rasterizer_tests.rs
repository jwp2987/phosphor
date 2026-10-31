use super::*;
use std::cell::Cell;

fn rect(w: i32, h: i32) -> RectI {
    RectI::new(vec2i(0, 0), vec2i(w, h))
}

#[test]
fn raster_bounds_are_computed_once_per_glyph_and_size() {
    let memo = RasterBoundsMemo::default();
    let computed = Cell::new(0);
    let compute = |bounds: RectI| {
        computed.set(computed.get() + 1);
        Ok(bounds)
    };

    let key = RasterBoundsMemo::key(FontId(1), 14., 42, Vector2F::splat(2.));
    // The glyph cache asks once for the atlas size and again for each subpixel variant.
    for _ in 0..4 {
        assert_eq!(
            memo.get_or_compute(key, || compute(rect(5, 7))).unwrap(),
            rect(5, 7)
        );
    }
    assert_eq!(computed.get(), 1);

    // A different glyph, size, scale or font is a different entry.
    for other in [
        RasterBoundsMemo::key(FontId(1), 14., 43, Vector2F::splat(2.)),
        RasterBoundsMemo::key(FontId(1), 15., 42, Vector2F::splat(2.)),
        RasterBoundsMemo::key(FontId(1), 14., 42, Vector2F::splat(1.)),
        RasterBoundsMemo::key(FontId(2), 14., 42, Vector2F::splat(2.)),
    ] {
        memo.get_or_compute(other, || compute(rect(1, 1))).unwrap();
    }
    assert_eq!(computed.get(), 5);
}

#[test]
fn raster_bounds_errors_are_not_memoized() {
    let memo = RasterBoundsMemo::default();
    let key = RasterBoundsMemo::key(FontId(1), 14., 42, Vector2F::splat(1.));

    assert!(
        memo.get_or_compute(key, || Err(anyhow!("no image")))
            .is_err()
    );
    assert_eq!(
        memo.get_or_compute(key, || Ok(rect(3, 3))).unwrap(),
        rect(3, 3)
    );
}
