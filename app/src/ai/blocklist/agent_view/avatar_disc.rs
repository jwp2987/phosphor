//! Avatar disc rendering shared by the orchestration pill bar, breadcrumbs, and
//! transcript surfaces. Pulled out of `orchestration_pill_bar.rs` because these
//! helpers are pure rendering with zero pill-bar state (no telemetry, no
//! `PillBarModel`), so other surfaces (and the pill bar itself) can import them
//! rather than redefine them.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pathfinder_color::ColorU;
use warp_core::ui::appearance::Appearance;
use warp_core::ui::theme::WarpTheme;
use warpui::elements::{
    Align, ConstrainedBox, Container, CornerRadius, Element, Empty, ParentElement, Radius, Stack,
    Text,
};
use warpui::fonts::{Properties, Weight};

use crate::ui_components::icons::Icon;

/// Stable palette used to color child agent avatars deterministically by name.
fn pill_palette(theme: &WarpTheme) -> [ColorU; 6] {
    [
        theme.ansi_fg_blue(),
        theme.ansi_fg_magenta(),
        theme.ansi_fg_cyan(),
        theme.ansi_fg_green(),
        theme.ansi_fg_yellow(),
        theme.ansi_fg_red(),
    ]
}

pub(crate) fn pill_avatar_color(name: &str, theme: &WarpTheme) -> ColorU {
    let palette = pill_palette(theme);
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let idx = (hasher.finish() as usize) % palette.len();
    palette[idx]
}

pub(crate) fn pill_initial(name: &str) -> char {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('A')
}

/// Renders the orchestrator avatar disc shared by pill, breadcrumb, and transcript
/// surfaces.
pub(crate) fn render_orchestrator_avatar_disc(
    size: f32,
    theme: &WarpTheme,
    appearance: &Appearance,
) -> Box<dyn Element> {
    render_avatar_disc(
        theme.ansi_fg_cyan(),
        AvatarGlyph::Icon(Icon::Agent),
        size,
        theme,
        appearance,
    )
}

/// Renders a child-agent avatar using the same deterministic-color + initial-letter
/// treatment as the orchestration pill bar.
pub(crate) fn render_agent_avatar_disc(
    name: &str,
    size: f32,
    theme: &WarpTheme,
    appearance: &Appearance,
) -> Box<dyn Element> {
    render_avatar_disc(
        pill_avatar_color(name, theme),
        AvatarGlyph::Letter(pill_initial(name)),
        size,
        theme,
        appearance,
    )
}

#[derive(Clone, Copy)]
pub(crate) enum AvatarGlyph {
    Letter(char),
    Icon(Icon),
}

/// Renders the avatar circle as a colored disc with a centered glyph (letter
/// or icon) on top. Uses `Stack` so the disc is a clean rounded square that
/// composites cleanly over the pill's own background without visual seams.
pub(crate) fn render_avatar_disc(
    avatar_color: ColorU,
    glyph: AvatarGlyph,
    size: f32,
    theme: &WarpTheme,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let disc = ConstrainedBox::new(
        Container::new(Empty::new().finish())
            .with_background_color(avatar_color)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(size / 2.)))
            .finish(),
    )
    .with_width(size)
    .with_height(size)
    .finish();
    let glyph_size = size * 0.625;

    let glyph_element: Box<dyn Element> = match glyph {
        AvatarGlyph::Letter(letter) => {
            Text::new(letter.to_string(), appearance.ui_font_family(), glyph_size)
                .with_color(theme.background().into_solid())
                .with_style(Properties {
                    weight: Weight::Bold,
                    ..Default::default()
                })
                // The default 1.2 ratio pads the text box with leading, so
                // centering the box leaves the letter's ink sitting high in
                // the disc. At 1.0 the box is the glyph, and centering it
                // centers what you can see.
                .with_line_height_ratio(1.)
                .finish()
        }
        AvatarGlyph::Icon(icon) => {
            ConstrainedBox::new(icon.to_warpui_icon(theme.background()).finish())
                .with_width(glyph_size)
                .with_height(glyph_size)
                .finish()
        }
    };

    let glyph_centered = ConstrainedBox::new(Align::new(glyph_element).finish())
        .with_width(size)
        .with_height(size)
        .finish();

    Stack::new()
        .with_child(disc)
        .with_child(glyph_centered)
        .finish()
}
