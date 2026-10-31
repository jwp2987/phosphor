use super::opaque_background;
use crate::elements::Fill;
use crate::scene::{Border, CornerRadius, Radius};
use crate::{ClipBounds, Scene};
use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::vec2f;

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
    assert_eq!(opaque_background(&scene, surface()), Some(BACKGROUND));

    // Logical bounds are compared in device pixels.
    let hidpi = scene_with_background(
        2.,
        RectF::new(vec2f(0., 0.), vec2f(400., 300.)),
        Fill::Solid(BACKGROUND),
    );
    assert_eq!(opaque_background(&hidpi, surface()), Some(BACKGROUND));

    // A rect larger than the surface still covers it.
    let oversized = scene_with_background(
        1.,
        RectF::new(vec2f(-5., -5.), vec2f(900., 700.)),
        Fill::Solid(BACKGROUND),
    );
    assert_eq!(opaque_background(&oversized, surface()), Some(BACKGROUND));
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
