use super::*;

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
