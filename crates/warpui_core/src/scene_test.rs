use pathfinder_geometry::vector::vec2f;

use crate::rendering;

use super::*;

#[test]
fn test_hit_rect_recording() {
    let mut scene = Scene::new(1., rendering::Config::default());
    assert_eq!(ZIndex::new(0), scene.z_index());

    scene.draw_rect_with_hit_recording(RectF::new(vec2f(0., 0.), vec2f(100., 100.)));
    assert_eq!(ZIndex::new(0), scene.z_index());
    assert!(!scene.is_covered(Point::new(10., 10., ZIndex::new(0))));

    scene.start_layer(ClipBounds::ActiveLayer);
    scene.draw_rect_with_hit_recording(RectF::new(vec2f(50., 50.), vec2f(100., 100.)));
    assert_eq!(ZIndex::new(1), scene.z_index());
    assert!(!scene.is_covered(Point::new(10., 10., ZIndex::new(0))));
    assert!(scene.is_covered(Point::new(60., 60., ZIndex::new(0))));

    scene.start_layer(ClipBounds::ActiveLayer);
    scene.draw_rect_with_hit_recording(RectF::new(vec2f(0., 0.), vec2f(100., 100.)));
    assert_eq!(ZIndex::new(2), scene.z_index());
    assert!(scene.is_covered(Point::new(10., 10., ZIndex::new(0))));
    assert!(scene.is_covered(Point::new(60., 60., ZIndex::new(1))));
}

#[test]
fn test_nested_clip_bounds_with_intersection() {
    let mut scene = Scene::new(1., rendering::Config::default());

    let bounds1 = RectF::new(Vector2F::zero(), Vector2F::new(10., 10.));
    scene.start_layer(ClipBounds::BoundedBy(bounds1));

    let bounds2 = RectF::new(Vector2F::new(5., 5.), Vector2F::new(10., 10.));
    scene.start_layer(ClipBounds::BoundedByActiveLayerAnd(bounds2));

    assert_eq!(
        scene.active_layer().clip_bounds,
        Some(RectF::new(Vector2F::new(5., 5.), Vector2F::new(5., 5.)))
    );
}

#[test]
fn test_nested_clip_bounds_with_no_intersection() {
    let mut scene = Scene::new(1., rendering::Config::default());

    let bounds1 = RectF::new(Vector2F::zero(), Vector2F::new(10., 10.));
    scene.start_layer(ClipBounds::BoundedBy(bounds1));

    let bounds2 = RectF::new(Vector2F::new(100., 100.), Vector2F::new(10., 10.));
    scene.start_layer(ClipBounds::BoundedByActiveLayerAnd(bounds2));

    // We should have explicit bounds for this layer.  (None represents a lack
    // of clipping, not clipping they layer down to nothingness.)
    assert!(scene.active_layer().clip_bounds.is_some());
    // The clip bounds should have an area of zero.
    assert!(scene.active_layer().clip_bounds.unwrap().is_empty());
}

#[test]
fn test_click_through_layer_does_not_cover_lower_layers() {
    let mut scene = Scene::new(1., rendering::Config::default());

    scene.start_layer(ClipBounds::ActiveLayer);
    scene.set_active_layer_click_through();
    scene.draw_rect_with_hit_recording(RectF::new(vec2f(0., 0.), vec2f(100., 100.)));

    assert!(!scene.is_covered(Point::new(10., 10., ZIndex::new(0))));
}

fn draw_sample_frame(scene: &mut Scene) {
    for i in 0..3 {
        scene.draw_rect_with_hit_recording(RectF::new(vec2f(i as f32, 0.), vec2f(1., 1.)));
    }
    scene.start_layer(ClipBounds::None);
    for i in 0..5 {
        scene.draw_glyph(vec2f(i as f32, 10.), i, FontId(0), 12., ColorU::black());
    }
    scene.start_overlay_layer(ClipBounds::None);
    scene.draw_rect_without_hit_recording(RectF::new(vec2f(0., 0.), vec2f(2., 2.)));
    scene.stop_layer();
    scene.stop_layer();
}

fn primitives(scene: &Scene) -> Vec<(Option<RectF>, Vec<RectF>, Vec<(GlyphKey, Vector2F)>)> {
    scene
        .layers()
        .map(|layer| {
            (
                layer.clip_bounds,
                layer.rects.iter().map(|rect| rect.bounds).collect(),
                layer
                    .glyphs
                    .iter()
                    .map(|glyph| (glyph.glyph_key, glyph.position))
                    .collect(),
            )
        })
        .collect()
}

/// Pre-allocating from the previous frame's hints changes only capacities: the same drawing
/// sequence records the same primitives, in the same layers, in the same order.
#[test]
fn capacity_hints_do_not_change_the_recorded_scene() {
    let mut first = Scene::new(1., rendering::Config::default());
    draw_sample_frame(&mut first);

    let mut hinted = Scene::with_capacity_hints(
        1.,
        rendering::Config::default(),
        first.capacity_hints(),
    );
    draw_sample_frame(&mut hinted);

    assert_eq!(primitives(&first), primitives(&hinted));
    assert_eq!(first.layer_count(), hinted.layer_count());
    assert_eq!(first.capacity_hints(), hinted.capacity_hints());
}

#[test]
fn capacity_hints_reserve_each_layer_up_front() {
    let mut first = Scene::new(1., rendering::Config::default());
    draw_sample_frame(&mut first);

    let mut hinted = Scene::with_capacity_hints(
        1.,
        rendering::Config::default(),
        first.capacity_hints(),
    );
    assert!(hinted.layers.first().rects.capacity() >= 3);
    hinted.start_layer(ClipBounds::None);
    assert!(hinted.active_layer().glyphs.capacity() >= 5);
    hinted.start_overlay_layer(ClipBounds::None);
    assert!(hinted.active_layer().rects.capacity() >= 1);

    // A scene with no history (or more layers than last frame) still works, just unreserved.
    let mut fresh = Scene::new(1., rendering::Config::default());
    fresh.start_layer(ClipBounds::None);
    fresh.start_layer(ClipBounds::None);
    fresh.start_layer(ClipBounds::None);
    assert_eq!(fresh.layer_count(), 4);
}
