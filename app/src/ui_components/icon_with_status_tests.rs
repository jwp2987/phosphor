use warp_core::ui::theme::Fill;

use super::{warp_agent_circle_colors, OZ_AMBIENT_BACKGROUND_COLOR};
use crate::themes::default_themes::{dark_theme, light_theme};

#[test]
fn local_warp_agent_circle_uses_white_glyph_on_black_for_dark_themes() {
    assert_eq!(
        warp_agent_circle_colors(&dark_theme(), false),
        (Fill::black(), Fill::white())
    );
}

#[test]
fn local_warp_agent_circle_uses_black_glyph_on_white_for_light_themes() {
    assert_eq!(
        warp_agent_circle_colors(&light_theme(), false),
        (Fill::white(), Fill::black())
    );
}

#[test]
fn ambient_warp_agent_circle_keeps_purple_background_in_all_themes() {
    let expected = (Fill::Solid(OZ_AMBIENT_BACKGROUND_COLOR), Fill::black());

    assert_eq!(warp_agent_circle_colors(&dark_theme(), true), expected);
    assert_eq!(warp_agent_circle_colors(&light_theme(), true), expected);
}

/// Strips whole-line `//` comments so the source scan below cannot be tripped
/// by prose. The comments explaining this fix name the old asset, and asserting
/// on raw source would make them a false failure.
fn code_lines(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Regression test for issue #636. The first-run panes rendered the pre-rename
/// mark, and the fix is not a path swap: routing `phosphor-logo.svg` through
/// `Icon` flat-tints its gradients away and the mark comes out a white
/// silhouette, so both call sites have to go through
/// [`super::render_phosphor_logo`]. Nothing in a unit test can observe which
/// element type a view built, so this asserts on the call sites themselves.
#[test]
fn first_run_panes_render_the_phosphor_mark_as_an_image() {
    for (name, source) in [
        (
            "welcome_view.rs",
            include_str!("../pane_group/pane/welcome_view.rs"),
        ),
        (
            "get_started_view.rs",
            include_str!("../pane_group/pane/get_started_view.rs"),
        ),
    ] {
        let code = code_lines(source);
        assert!(
            !code.contains("warp-logo-neutral.svg"),
            "{name} is back to rendering the pre-rename mark"
        );
        assert!(
            code.contains("render_phosphor_logo()"),
            "{name} no longer goes through render_phosphor_logo, so the mark \
             may be flat-tinted into a white silhouette"
        );
    }
}

/// The reason `render_phosphor_logo` must use `Image`: `Icon` renders its asset
/// with a single fill, which erases these.
#[test]
fn the_phosphor_mark_is_gradient_filled() {
    let svg = include_str!("../../assets/bundled/svg/phosphor-logo.svg");
    assert!(
        svg.contains("linearGradient"),
        "{} no longer carries gradients; if that is deliberate, revisit whether \
         it still needs the Image branch",
        super::PHOSPHOR_LOGO_PATH
    );
}
