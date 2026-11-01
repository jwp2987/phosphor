use super::*;
use crate::fonts::FontId;
use crate::fonts::canvas::{Canvas, RasterFormat};
use ordered_float::OrderedFloat;
use pathfinder_geometry::vector::vec2i;
use std::cell::Cell;

fn glyph_key(glyph_id: u32) -> GlyphKey {
    GlyphKey {
        glyph_id,
        font_id: FontId(0),
        font_size: OrderedFloat(14.),
    }
}

fn rasterized() -> RasterizedGlyph {
    RasterizedGlyph {
        canvas: Canvas {
            pixels: vec![0; 4 * 4 * 4],
            size: vec2i(4, 4),
            row_stride: 16,
            format: RasterFormat::Rgba32,
        },
        is_emoji: false,
    }
}

/// Drives `GlyphCache::get` with callbacks that count how often the font is asked for bounds and
/// pixels. Glyph id 0 has no outline (zero-size bounds), every other glyph is 4x4.
struct Harness {
    cache: GlyphCache<()>,
    bounds_calls: Cell<usize>,
    rasterize_calls: Cell<usize>,
}

impl Harness {
    fn new() -> Self {
        Self {
            cache: GlyphCache::new(Default::default()),
            bounds_calls: Cell::new(0),
            rasterize_calls: Cell::new(0),
        }
    }

    fn get(&mut self, glyph_id: u32, subpixel: SubpixelAlignment) -> Option<GlyphTextureOffset> {
        let bounds_calls = &self.bounds_calls;
        let rasterize_calls = &self.rasterize_calls;
        self.cache
            .get(
                glyph_key(glyph_id),
                1.,
                subpixel,
                &|_| (),
                &|_, _, _| {},
                &|key, _, _| {
                    bounds_calls.set(bounds_calls.get() + 1);
                    Ok(if key.glyph_id == 0 {
                        RectI::new(vec2i(0, 0), vec2i(0, 0))
                    } else {
                        RectI::new(vec2i(0, 0), vec2i(4, 4))
                    })
                },
                &|_, _, _, _, _| {
                    rasterize_calls.set(rasterize_calls.get() + 1);
                    Ok(rasterized())
                },
            )
            .unwrap()
    }
}

#[test]
fn glyph_is_measured_and_rasterized_once() {
    let mut h = Harness::new();
    let subpixel = SubpixelAlignment::new(Vector2F::zero());

    let first = h.get(7, subpixel).expect("glyph has pixels");
    for _ in 0..5 {
        let again = h.get(7, subpixel).expect("glyph has pixels");
        assert_eq!(again.texture_id.as_usize(), first.texture_id.as_usize());
        assert_eq!(again.raster_bounds, first.raster_bounds);
    }
    assert_eq!(h.bounds_calls.get(), 1);
    assert_eq!(h.rasterize_calls.get(), 1);

    // Another subpixel variant is a new entry.
    h.get(7, SubpixelAlignment::new(Vector2F::new(0.66, 0.)));
    assert_eq!(h.rasterize_calls.get(), 2);
}

#[test]
fn zero_size_glyph_is_cached() {
    let mut h = Harness::new();
    let subpixel = SubpixelAlignment::new(Vector2F::zero());

    for _ in 0..5 {
        assert!(h.get(0, subpixel).is_none());
    }
    // Measured once, then answered from the cache; never rasterized.
    assert_eq!(h.bounds_calls.get(), 1);
    assert_eq!(h.rasterize_calls.get(), 0);
}

/// With a tiny budget a frame that needs several new glyphs rasterises only part of them, flags
/// that another frame is needed, and the following frames complete the rest (#1168 F6).
#[test]
fn rasterisation_over_budget_spills_to_following_frames() {
    let mut h = Harness::new();
    h.cache.raster_budget = Some(Duration::ZERO);
    let subpixel = SubpixelAlignment::new(Vector2F::zero());

    h.cache.begin_frame();
    let drawn = (1..=3).filter(|id| h.get(*id, subpixel).is_some()).count();
    assert_eq!(drawn, 1, "only the guaranteed first glyph fits the budget");
    assert_eq!(h.rasterize_calls.get(), 1);
    assert!(h.cache.has_deferred());

    h.cache.begin_frame();
    let drawn = (1..=3).filter(|id| h.get(*id, subpixel).is_some()).count();
    assert_eq!(drawn, 2);
    assert!(h.cache.has_deferred());

    h.cache.begin_frame();
    let drawn = (1..=3).filter(|id| h.get(*id, subpixel).is_some()).count();
    assert_eq!(drawn, 3, "every glyph is eventually drawn");
    assert!(!h.cache.has_deferred(), "nothing left, so no further frame");
    assert_eq!(h.rasterize_calls.get(), 3);

    // An idle frame with everything cached requests nothing.
    h.cache.begin_frame();
    assert_eq!(
        (1..=3).filter(|id| h.get(*id, subpixel).is_some()).count(),
        3
    );
    assert!(!h.cache.has_deferred());
}

/// Without a budget (the kill switch) one frame rasterises everything it needs.
#[test]
fn no_budget_rasterises_everything_in_one_frame() {
    let mut h = Harness::new();
    h.cache.raster_budget = None;
    let subpixel = SubpixelAlignment::new(Vector2F::zero());
    h.cache.begin_frame();
    assert_eq!(
        (1..=5).filter(|id| h.get(*id, subpixel).is_some()).count(),
        5
    );
    assert!(!h.cache.has_deferred());
}

/// A deferral left by a render that never reached `take_deferred_redraw` (a failed draw) must not
/// make the NEXT window's render ask for a redraw (#1379).
#[test]
fn deferred_redraw_flag_is_scoped_to_one_window_render() {
    let mut h = Harness::new();
    h.cache.raster_budget = Some(Duration::ZERO);
    let subpixel = SubpixelAlignment::new(Vector2F::zero());

    // Window A renders, defers, and bails out before taking the flag.
    begin_window_render();
    h.cache.begin_frame();
    let _ = (1..=3).filter(|id| h.get(*id, subpixel).is_some()).count();
    assert!(h.cache.has_deferred());

    // Window B renders with everything cached: it must not inherit A's flag.
    begin_window_render();
    assert!(!take_deferred_redraw());

    // A render that does defer is still reported once, then cleared.
    h.cache.begin_frame();
    let _ = (1..=3).filter(|id| h.get(*id, subpixel).is_some()).count();
    assert!(take_deferred_redraw());
    assert!(!take_deferred_redraw());
}
