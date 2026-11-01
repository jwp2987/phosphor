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

fn paint_first_part(scene: &mut Scene) {
    scene
        .draw_rect_with_hit_recording(RectF::new(vec2f(0., 0.), vec2f(10., 10.)))
        .with_background(Fill::Solid(ColorU::new(1, 1, 1, 255)));
    scene.draw_glyph(vec2f(1., 1.), 1, FontId(0), 12., ColorU::black());
}

fn paint_deferred_part(scene: &mut Scene) {
    scene
        .draw_rect_without_hit_recording(RectF::new(vec2f(20., 0.), vec2f(5., 5.)))
        .with_background(Fill::Solid(ColorU::new(2, 2, 2, 255)));
    scene.draw_rect_with_hit_recording(RectF::new(vec2f(30., 0.), vec2f(5., 5.)));
    for i in 0..3 {
        scene.draw_glyph(vec2f(20. + i as f32, 1.), 10 + i, FontId(1), 12., ColorU::black());
    }
}

fn paint_last_part(scene: &mut Scene) {
    scene.draw_rect_with_hit_recording(RectF::new(vec2f(40., 0.), vec2f(1., 1.)));
    scene.draw_glyph(vec2f(40., 1.), 99, FontId(0), 12., ColorU::black());
}

fn colors(scene: &Scene) -> Vec<Vec<Fill>> {
    scene
        .layers()
        .map(|layer| layer.rects.iter().map(|rect| rect.background).collect())
        .collect()
}

/// Painting the middle part later into a scratch scene and splicing it at the recorded point
/// records the same primitives, in the same order, as painting everything directly.
#[test]
fn splice_at_orders_deferred_primitives_as_if_painted_in_place() {
    let mut direct = Scene::new(1., rendering::Config::default());
    paint_first_part(&mut direct);
    paint_deferred_part(&mut direct);
    paint_last_part(&mut direct);

    let mut deferred = Scene::new(1., rendering::Config::default());
    paint_first_part(&mut deferred);
    let point = deferred.insertion_point();
    paint_last_part(&mut deferred);

    let mut scratch = deferred.scratch();
    paint_deferred_part(&mut scratch);
    deferred.splice_at(point, scratch);

    assert_eq!(primitives(&direct), primitives(&deferred));
    assert_eq!(colors(&direct), colors(&deferred));
    assert_eq!(
        direct.layers.first().hit_map.size(),
        deferred.layers.first().hit_map.size()
    );
}

/// Two deferred parts in one layer, and one inside a started layer: splicing in reverse order of
/// recording keeps every earlier point valid.
#[test]
fn splice_at_in_reverse_order_handles_several_points_and_sub_layers() {
    let mut direct = Scene::new(1., rendering::Config::default());
    paint_first_part(&mut direct);
    paint_deferred_part(&mut direct);
    paint_last_part(&mut direct);
    paint_deferred_part(&mut direct);
    direct.start_layer(ClipBounds::None);
    paint_deferred_part(&mut direct);
    paint_last_part(&mut direct);
    direct.stop_layer();

    let mut deferred = Scene::new(1., rendering::Config::default());
    paint_first_part(&mut deferred);
    let first = deferred.insertion_point();
    paint_last_part(&mut deferred);
    let second = deferred.insertion_point();
    deferred.start_layer(ClipBounds::None);
    let third = deferred.insertion_point();
    paint_last_part(&mut deferred);
    deferred.stop_layer();

    for point in [third, second, first] {
        let mut scratch = deferred.scratch();
        paint_deferred_part(&mut scratch);
        deferred.splice_at(point, scratch);
    }

    assert_eq!(primitives(&direct), primitives(&deferred));
    assert_eq!(colors(&direct), colors(&deferred));
}

fn rect_with(background: Fill, border: Border) -> Rect {
    Rect {
        bounds: RectF::new(vec2f(0., 0.), vec2f(10., 10.)),
        background,
        border,
        ..Default::default()
    }
}

#[test]
fn a_rect_without_alpha_and_without_a_drawn_border_draws_nothing() {
    assert!(rect_with(Fill::None, Border::default()).draws_nothing());
    assert!(rect_with(Fill::Solid(ColorU::new(9, 9, 9, 0)), Border::default()).draws_nothing());
    assert!(rect_with(
        Fill::Gradient {
            start: vec2f(0., 0.),
            end: vec2f(1., 1.),
            start_color: ColorU::new(1, 2, 3, 0),
            end_color: ColorU::new(4, 5, 6, 0),
        },
        Border::default(),
    )
    .draws_nothing());
    // A border colour with no alpha, or a border with no width on any side, adds nothing.
    assert!(rect_with(Fill::None, Border::all(2.)).draws_nothing());
    let mut no_sides = Border::all(2.);
    no_sides.color = Fill::Solid(ColorU::new(255, 0, 0, 255));
    no_sides.top = false;
    no_sides.right = false;
    no_sides.bottom = false;
    no_sides.left = false;
    assert!(rect_with(Fill::None, no_sides).draws_nothing());
}

#[test]
fn a_rect_with_any_visible_alpha_or_border_is_drawn() {
    assert!(!rect_with(Fill::Solid(ColorU::new(0, 0, 0, 1)), Border::default()).draws_nothing());
    assert!(!rect_with(
        Fill::Gradient {
            start: vec2f(0., 0.),
            end: vec2f(1., 1.),
            start_color: ColorU::new(0, 0, 0, 0),
            end_color: ColorU::new(0, 0, 0, 255),
        },
        Border::default(),
    )
    .draws_nothing());
    let mut visible = Border::all(1.);
    visible.color = Fill::Solid(ColorU::new(255, 255, 255, 255));
    assert!(!rect_with(Fill::None, visible).draws_nothing());
    let mut one_side = Border::all(1.);
    one_side.color = Fill::Solid(ColorU::new(255, 255, 255, 80));
    one_side.top = false;
    one_side.left = false;
    one_side.right = false;
    assert!(!rect_with(Fill::None, one_side).draws_nothing());
}

#[test]
fn zero_radii_are_square_and_anything_else_is_not() {
    assert!(CornerRadius::default().is_square());
    assert!(CornerRadius::with_all(Radius::Pixels(0.)).is_square());
    assert!(CornerRadius::with_top(Radius::Pixels(0.)).is_square());
    assert!(!CornerRadius::with_all(Radius::Pixels(4.)).is_square());
    assert!(!CornerRadius::with_bottom_right(Radius::Pixels(0.5)).is_square());
    assert!(!CornerRadius::with_all(Radius::Percentage(0.)).is_square());
}

#[test]
fn stats_count_primitives_and_areas_in_device_pixels() {
    let mut scene = Scene::new(2., rendering::Config::default());
    // 10x10 logical solid square rect: drawn, 400 device px.
    scene
        .draw_rect_without_hit_recording(RectF::new(vec2f(0., 0.), vec2f(10., 10.)))
        .with_background(ColorU::new(1, 2, 3, 255));
    // Fully transparent: skipped, not counted as drawn.
    scene.draw_rect_without_hit_recording(RectF::new(vec2f(0., 0.), vec2f(500., 500.)));
    // Large rounded rect with a shadow.
    scene
        .draw_rect_without_hit_recording(RectF::new(vec2f(0., 0.), vec2f(300., 300.)))
        .with_background(ColorU::new(1, 2, 3, 255))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
        .with_drop_shadow(DropShadow::new_with_standard_offset_and_spread(
            ColorU::black(),
        ));
    scene.start_layer(ClipBounds::BoundedBy(RectF::new(
        vec2f(0., 0.),
        vec2f(10., 10.),
    )));
    scene
        .draw_rect_without_hit_recording(RectF::new(vec2f(0., 0.), vec2f(100., 100.)))
        .with_background(ColorU::new(1, 2, 3, 255));

    let stats = scene.stats();
    assert_eq!(stats.layers, 2);
    assert_eq!(stats.clipped_layers, 1);
    assert_eq!(stats.rects, 3);
    assert_eq!(stats.skipped_rects, 1);
    assert_eq!(stats.rounded_rects, 1);
    assert_eq!(stats.shadows, 1);
    assert_eq!(stats.large_rects, 1);
    // 400 + 360000 + (clipped to 10x10 logical) 400
    assert_eq!(stats.rect_area_px, 400. + 360_000. + 400.);
    assert_eq!(stats.shadow_area_px, 360. * 360. * 4.);
}

#[test]
fn fingerprint_is_equal_for_identical_scenes_and_differs_on_content() {
    let build = |color: ColorU| {
        let mut scene = Scene::new(1., rendering::Config::default());
        scene
            .draw_rect_without_hit_recording(RectF::new(vec2f(0., 0.), vec2f(10., 10.)))
            .with_background(color);
        scene
    };
    assert_eq!(
        build(ColorU::new(1, 2, 3, 255)).fingerprint(),
        build(ColorU::new(1, 2, 3, 255)).fingerprint()
    );
    assert_ne!(
        build(ColorU::new(1, 2, 3, 255)).fingerprint(),
        build(ColorU::new(1, 2, 4, 255)).fingerprint()
    );
}
