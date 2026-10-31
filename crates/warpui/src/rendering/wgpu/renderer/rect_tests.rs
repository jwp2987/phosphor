use super::{FAST_PATH_OFF, FAST_PATH_ON, encode_layer, shader_source_with, shader_types};
use std::borrow::Cow;
use crate::elements::Fill;
use crate::rendering;
use crate::Scene;
use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::vec2f;

fn sample_scene() -> Scene {
    let mut scene = Scene::new(2., rendering::Config::default());
    scene
        .draw_rect_with_hit_recording(RectF::new(vec2f(1., 2.), vec2f(30., 40.)))
        .with_background(Fill::Solid(ColorU::new(10, 20, 30, 255)));
    scene
        .draw_rect_without_hit_recording(RectF::new(vec2f(5., 5.), vec2f(1., 1.)))
        .with_drop_shadow(Default::default());
    scene.start_layer(crate::ClipBounds::None);
    scene
        .draw_rect_without_hit_recording(RectF::new(vec2f(0., 0.), vec2f(8., 8.)))
        .with_background(Fill::Solid(ColorU::new(1, 2, 3, 4)));
    scene.stop_layer();
    scene
}

fn encode_all(scene: &Scene, out: &mut Vec<shader_types::RectData>) -> Vec<(usize, usize)> {
    scene
        .layers()
        .filter_map(|layer| encode_layer(layer, scene.scale_factor(), out))
        .map(|state| (state.start_offset, state.len))
        .collect()
}

/// Encoding into a vector kept from the last frame (cleared, capacity retained) must produce
/// exactly the bytes a fresh vector would, with the same per-layer offsets.
#[test]
fn reused_instance_vector_encodes_the_same_bytes_as_a_fresh_one() {
    let scene = sample_scene();

    let mut fresh = Vec::new();
    let fresh_layers = encode_all(&scene, &mut fresh);

    let mut reused = Vec::new();
    encode_all(&scene, &mut reused);
    let capacity_after_first_frame = reused.capacity();
    reused.clear();
    let reused_layers = encode_all(&scene, &mut reused);

    assert_eq!(fresh_layers, reused_layers);
    // Two rects, one of them with a drop shadow (two instances), then one more in layer two.
    assert_eq!(fresh_layers, vec![(0, 3), (3, 1)]);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&fresh),
        bytemuck::cast_slice::<_, u8>(&reused)
    );
    assert_eq!(reused.capacity(), capacity_after_first_frame);
}

#[test]
fn layers_without_rects_add_no_instances() {
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.start_layer(crate::ClipBounds::None);
    let mut out = Vec::new();
    assert!(encode_all(&scene, &mut out).is_empty());
    assert!(out.is_empty());
}

const SHADER: &str = include_str!("../shaders/rect_shader.wgsl");

#[test]
fn shader_has_exactly_one_fast_path_switch_line() {
    // The knob works by replacing this line; if it is reworded the knob silently does nothing.
    assert_eq!(SHADER.matches(FAST_PATH_ON).count(), 1);
}

#[test]
fn fast_path_is_on_by_default_and_off_on_request() {
    let on = shader_source_with(SHADER, false);
    assert!(on.contains(FAST_PATH_ON));
    assert!(matches!(on, Cow::Borrowed(_)));

    let off = shader_source_with(SHADER, true);
    assert!(!off.contains(FAST_PATH_ON));
    assert!(off.contains(FAST_PATH_OFF));
}
