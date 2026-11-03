use super::{OpaqueBackground, opaque_background};
use crate::elements::Fill;
use crate::scene::{Border, CornerRadius, Radius};
use crate::{ClipBounds, Scene};
use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::vec2f;

fn background(layer: usize) -> OpaqueBackground {
    OpaqueBackground {
        layer,
        color: BACKGROUND,
    }
}

const BACKGROUND: ColorU = ColorU {
    r: 20,
    g: 30,
    b: 40,
    a: 255,
};

fn surface() -> pathfinder_geometry::vector::Vector2F {
    vec2f(800., 600.)
}

fn scene_with_background(scale_factor: f32, bounds: RectF, fill: Fill) -> Scene {
    let mut scene = Scene::new(scale_factor, crate::rendering::Config::default());
    scene
        .draw_rect_with_hit_recording(bounds)
        .with_background(fill);
    scene
        .draw_rect_without_hit_recording(RectF::new(vec2f(10., 10.), vec2f(50., 20.)))
        .with_background(Fill::Solid(ColorU::new(1, 2, 3, 255)));
    scene
}

#[test]
fn a_full_surface_opaque_solid_first_rect_is_the_clear_colour() {
    let scene = scene_with_background(
        1.,
        RectF::new(vec2f(0., 0.), vec2f(800., 600.)),
        Fill::Solid(BACKGROUND),
    );
    assert_eq!(opaque_background(&scene, surface()), Some(background(0)));

    // Logical bounds are compared in device pixels.
    let hidpi = scene_with_background(
        2.,
        RectF::new(vec2f(0., 0.), vec2f(400., 300.)),
        Fill::Solid(BACKGROUND),
    );
    assert_eq!(opaque_background(&hidpi, surface()), Some(background(0)));

    // A rect larger than the surface still covers it.
    let oversized = scene_with_background(
        1.,
        RectF::new(vec2f(-5., -5.), vec2f(900., 700.)),
        Fill::Solid(BACKGROUND),
    );
    assert_eq!(
        opaque_background(&oversized, surface()),
        Some(background(0))
    );
}

#[test]
fn rects_the_shader_would_draw_differently_from_a_clear_are_not_used() {
    let full = RectF::new(vec2f(0., 0.), vec2f(800., 600.));

    let translucent = scene_with_background(1., full, Fill::Solid(ColorU::new(20, 30, 40, 128)));
    assert_eq!(opaque_background(&translucent, surface()), None);

    let gradient = scene_with_background(
        1.,
        full,
        Fill::Gradient {
            start: vec2f(0., 0.),
            start_color: BACKGROUND,
            end: vec2f(1., 1.),
            end_color: ColorU::new(0, 0, 0, 255),
        },
    );
    assert_eq!(opaque_background(&gradient, surface()), None);

    let partial = scene_with_background(
        1.,
        RectF::new(vec2f(0., 0.), vec2f(800., 599.)),
        Fill::Solid(BACKGROUND),
    );
    assert_eq!(opaque_background(&partial, surface()), None);

    let mut rounded = Scene::new(1., crate::rendering::Config::default());
    rounded
        .draw_rect_without_hit_recording(full)
        .with_background(Fill::Solid(BACKGROUND))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
    assert_eq!(opaque_background(&rounded, surface()), None);

    let mut bordered = Scene::new(1., crate::rendering::Config::default());
    bordered
        .draw_rect_without_hit_recording(full)
        .with_background(Fill::Solid(BACKGROUND))
        .with_border(Border::all(1.));
    assert_eq!(opaque_background(&bordered, surface()), None);

    let mut shadowed = Scene::new(1., crate::rendering::Config::default());
    shadowed
        .draw_rect_without_hit_recording(full)
        .with_background(Fill::Solid(BACKGROUND))
        .with_drop_shadow(Default::default());
    assert_eq!(opaque_background(&shadowed, surface()), None);

    let mut clipped = Scene::new(1., crate::rendering::Config::default());
    clipped.start_layer(ClipBounds::BoundedBy(RectF::new(
        vec2f(0., 0.),
        vec2f(100., 100.),
    )));
    clipped.stop_layer();
    // The base layer has no clip, but its first rect must be in the base layer: here it is not.
    assert_eq!(opaque_background(&clipped, surface()), None);

    let empty = Scene::new(1., crate::rendering::Config::default());
    assert_eq!(opaque_background(&empty, surface()), None);
}

/// The window background the app emits carries an explicit radius of zero on every corner
/// (`CornerRadius::with_all(Radius::Pixels(0.))`), which is not equal to the default but draws the
/// same; it must still become the clear colour.
#[test]
fn an_explicit_zero_radius_is_still_the_clear_colour() {
    let full = RectF::new(vec2f(0., 0.), vec2f(800., 600.));
    let mut zero_radius = Scene::new(1., crate::rendering::Config::default());
    zero_radius
        .draw_rect_without_hit_recording(full)
        .with_background(Fill::Solid(BACKGROUND))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(0.)));
    assert_eq!(
        opaque_background(&zero_radius, surface()),
        Some(background(0))
    );
}

fn full() -> RectF {
    RectF::new(vec2f(0., 0.), vec2f(800., 600.))
}

fn background_fill() -> Fill {
    Fill::Solid(BACKGROUND)
}

/// The app's window background is not in the base layer (which is empty); it is the first rect of
/// the first layer that has any content.
#[test]
fn leading_empty_layers_and_rects_that_draw_nothing_are_looked_past() {
    let mut scene = Scene::new(1., crate::rendering::Config::default());
    scene.start_layer(ClipBounds::None);
    scene.stop_layer();
    // A layer whose only rect has no fill and no border.
    scene.start_layer(ClipBounds::None);
    scene.draw_rect_without_hit_recording(full());
    scene.stop_layer();
    scene.start_layer(ClipBounds::None);
    // Rects that draw nothing in front of the background do not move it from instance 0.
    scene.draw_rect_without_hit_recording(full());
    scene
        .draw_rect_without_hit_recording(full())
        .with_background(background_fill());
    scene
        .draw_rect_without_hit_recording(RectF::new(vec2f(10., 10.), vec2f(5., 5.)))
        .with_background(Fill::Solid(ColorU::new(1, 2, 3, 255)));
    scene.stop_layer();
    let found = opaque_background(&scene, surface()).expect("a background");
    assert_eq!(found.color, BACKGROUND);
    let layers: Vec<_> = scene.layers().collect();
    assert!(
        layers[found.layer].rects.len() == 3,
        "found layer {}",
        found.layer
    );
}

#[test]
fn anything_drawn_before_the_background_rect_blocks_the_clear() {
    // A glyph-less, rect-less layer is fine; an earlier icon or a visible earlier rect is not.
    let mut earlier_rect = Scene::new(1., crate::rendering::Config::default());
    earlier_rect
        .draw_rect_without_hit_recording(RectF::new(vec2f(0., 0.), vec2f(5., 5.)))
        .with_background(Fill::Solid(ColorU::new(9, 9, 9, 255)));
    earlier_rect.start_layer(ClipBounds::None);
    earlier_rect
        .draw_rect_without_hit_recording(full())
        .with_background(background_fill());
    earlier_rect.stop_layer();
    // The first drawn rect is the small one, which is not a background.
    assert_eq!(opaque_background(&earlier_rect, surface()), None);

    // A shadow is drawn before the rect that casts it, so a leading shadow is the first draw.
    let mut shadow_first = Scene::new(1., crate::rendering::Config::default());
    shadow_first
        .draw_rect_without_hit_recording(RectF::new(vec2f(10., 10.), vec2f(5., 5.)))
        .with_drop_shadow(Default::default());
    shadow_first
        .draw_rect_without_hit_recording(full())
        .with_background(background_fill());
    assert_eq!(opaque_background(&shadow_first, surface()), None);
}
