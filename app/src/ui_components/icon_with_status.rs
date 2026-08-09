use pathfinder_color::ColorU;
use pathfinder_geometry::vector::vec2f;
use warp_core::ui::icons::Icon as WarpIcon;
use warp_core::ui::theme::color::internal_colors;
use warp_core::ui::theme::{ColorScheme, Fill as WarpThemeFill, WarpTheme};
use warpui::{
    assets::asset_cache::AssetSource,
    elements::{
        CacheOption, ChildAnchor, ConstrainedBox, Container, CornerRadius, Element,
        Fill as ElementFill, Image, OffsetPositioning, ParentAnchor, ParentElement,
        ParentOffsetBounds, Radius, Stack,
    },
};

use crate::ai::agent::conversation::ConversationStatus;
use crate::terminal::CLIAgent;
use crate::themes::theme::Fill as ThemeFill;

/// Background color used for the Oz agent's circle when it is running in an ambient (cloud)
/// run. Matches the Oz brand purple used in the cloud-mode design spec.
const OZ_AMBIENT_BACKGROUND_COLOR: ColorU = ColorU {
    r: 203,
    g: 176,
    b: 247,
    a: 255,
};

/// Sizing configuration for the icon circle and its status badge.
pub(crate) struct IconWithStatusSizing {
    pub(crate) icon_size: f32,
    pub(crate) padding: f32,
    pub(crate) badge_icon_size: f32,
    pub(crate) badge_padding: f32,
    /// The overall constrained size for the stack.
    /// When set, overrides the default `icon_size + padding * 2`.
    pub(crate) overall_size_override: Option<f32>,
    /// Offset of the status badge from the bottom-right corner of the circle.
    /// Positive x pushes right, positive y pushes down.
    pub(crate) badge_offset: (f32, f32),
}

const DEEPSEEK_LOGO_PATH: &str = "bundled/svg/deepseek.svg";
const ANTIGRAVITY_LOGO_PATH: &str = "bundled/svg/antigravity.svg";
const OMP_LOGO_PATH: &str = "bundled/svg/omp.svg";

pub(crate) fn render_cli_agent_logo(
    agent: CLIAgent,
    icon_color: WarpThemeFill,
    fallback_icon_color: WarpThemeFill,
) -> Box<dyn Element> {
    let multi_color_logo_path = match agent {
        CLIAgent::DeepSeek => Some(DEEPSEEK_LOGO_PATH),
        CLIAgent::Antigravity => Some(ANTIGRAVITY_LOGO_PATH),
        CLIAgent::Omp => Some(OMP_LOGO_PATH),
        _ => None,
    };
    if let Some(path) = multi_color_logo_path {
        Image::new(
            AssetSource::Bundled { path },
            CacheOption::BySize,
        )
        .finish()
    } else {
        agent
            .icon()
            .map(|icon| icon.to_warpui_icon(icon_color).finish())
            .unwrap_or_else(|| {
                WarpIcon::Terminal
                    .to_warpui_icon(fallback_icon_color)
                    .finish()
            })
    }
}

/// What to render inside the circle.
pub(crate) enum IconWithStatusVariant {
    /// A generic icon with a given color on an overlay background.
    Neutral {
        icon: WarpIcon,
        icon_color: WarpThemeFill,
    },
    /// A pre-built icon element on an overlay background.
    NeutralElement { icon_element: Box<dyn Element> },
    /// An Oz agent icon on the theme background.
    OzAgent {
        status: Option<ConversationStatus>,
        is_ambient: bool,
    },
    /// A CLI agent icon on the agent's brand color background.
    CLIAgent {
        agent: CLIAgent,
        status: Option<ConversationStatus>,
        /// Whether this run is executing in an ambient (cloud) context rather than locally.
        /// Not currently used for CLI-agent color treatment (only `OzAgent` gets the
        /// ambient-purple background, see `warp_agent_circle_colors`) but is carried on the
        /// variant so cross-surface callers (`ui_components::agent_icon`) have a single
        /// consistent shape to construct regardless of which branch of the icon they render.
        is_ambient: bool,
    },
}

/// Renders an icon inside a circle with an optional status badge overlay.
pub(crate) fn render_icon_with_status(
    variant: IconWithStatusVariant,
    sizing: &IconWithStatusSizing,
    theme: &WarpTheme,
    badge_ring_background: WarpThemeFill,
) -> Box<dyn Element> {
    let sub_text = theme.sub_text_color(theme.background());

    match variant {
        IconWithStatusVariant::Neutral { icon, icon_color } => {
            let inner = ConstrainedBox::new(icon.to_warpui_icon(icon_color).finish())
                .with_width(sizing.icon_size)
                .with_height(sizing.icon_size)
                .finish();
            Container::new(inner)
                .with_uniform_padding(sizing.padding)
                .with_background(internal_colors::fg_overlay_2(theme))
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                    (sizing.icon_size + sizing.padding * 2.) / 2.,
                )))
                .finish()
        }
        IconWithStatusVariant::NeutralElement { icon_element } => {
            let inner = ConstrainedBox::new(icon_element)
                .with_width(sizing.icon_size)
                .with_height(sizing.icon_size)
                .finish();
            Container::new(inner)
                .with_uniform_padding(sizing.padding)
                .with_background(internal_colors::fg_overlay_2(theme))
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                    (sizing.icon_size + sizing.padding * 2.) / 2.,
                )))
                .finish()
        }
        IconWithStatusVariant::OzAgent { status, is_ambient } => {
            let icon = if is_ambient {
                WarpIcon::OzCloud
            } else {
                WarpIcon::Oz
            };
            let (circle_background, glyph_color) = warp_agent_circle_colors(theme, is_ambient);
            let inner = ConstrainedBox::new(icon.to_warpui_icon(glyph_color).finish())
                .with_width(sizing.icon_size)
                .with_height(sizing.icon_size)
                .finish();
            let circle = Container::new(inner)
                .with_uniform_padding(sizing.padding)
                .with_background(circle_background)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                    (sizing.icon_size + sizing.padding * 2.) / 2.,
                )))
                .finish();
            render_with_optional_status_badge(
                circle,
                status.as_ref(),
                sizing,
                theme,
                badge_ring_background,
            )
        }
        IconWithStatusVariant::CLIAgent { agent, status, .. } => {
            let brand_color = agent
                .brand_color()
                .unwrap_or(ColorU::new(100, 100, 100, 255));
            let icon_color = agent.brand_icon_color();
            let icon_element =
                render_cli_agent_logo(agent, WarpThemeFill::Solid(icon_color), sub_text);
            let inner = ConstrainedBox::new(icon_element)
                .with_width(sizing.icon_size)
                .with_height(sizing.icon_size)
                .finish();
            let background: ElementFill =
                if matches!(agent, CLIAgent::DeepSeek | CLIAgent::Antigravity | CLIAgent::Omp) {
                    theme.background().into()
                } else {
                    ThemeFill::Solid(brand_color).into()
                };
            let circle = Container::new(inner)
                .with_uniform_padding(sizing.padding)
                .with_background(background)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                    (sizing.icon_size + sizing.padding * 2.) / 2.,
                )))
                .finish();
            render_with_optional_status_badge(
                circle,
                status.as_ref(),
                sizing,
                theme,
                badge_ring_background,
            )
        }
    }
}

/// Derives the Oz agent circle's background and glyph colors for the given theme and
/// ambient-ness. Local (non-ambient) runs flip black-on-white vs white-on-black to match the
/// theme's light/dark scheme; ambient (cloud) runs always keep the Oz brand purple background
/// with a black glyph, regardless of theme.
fn warp_agent_circle_colors(theme: &WarpTheme, is_ambient: bool) -> (WarpThemeFill, WarpThemeFill) {
    if is_ambient {
        return (
            WarpThemeFill::Solid(OZ_AMBIENT_BACKGROUND_COLOR),
            WarpThemeFill::Solid(ColorU::black()),
        );
    }
    match theme.inferred_color_scheme() {
        ColorScheme::LightOnDark => (WarpThemeFill::black(), WarpThemeFill::white()),
        ColorScheme::DarkOnLight => (WarpThemeFill::white(), WarpThemeFill::black()),
    }
}

/// Adds a status badge with a cutout ring to the bottom-right of the circle.
fn render_with_optional_status_badge(
    circle: Box<dyn Element>,
    status: Option<&ConversationStatus>,
    sizing: &IconWithStatusSizing,
    theme: &WarpTheme,
    badge_ring_background: WarpThemeFill,
) -> Box<dyn Element> {
    let Some(status) = status else {
        return circle;
    };
    let (icon, color) = status.status_icon_and_color(theme);
    let badge_icon = ConstrainedBox::new(icon.to_warpui_icon(WarpThemeFill::Solid(color)).finish())
        .with_width(sizing.badge_icon_size)
        .with_height(sizing.badge_icon_size)
        .finish();
    let badge = Container::new(badge_icon)
        .with_uniform_padding(sizing.badge_padding)
        .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
        .finish();
    // Cutout ring that visually separates the badge from the circle.
    let badge_with_ring = Container::new(badge)
        .with_uniform_padding(sizing.badge_padding)
        .with_background(badge_ring_background)
        .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
        .finish();

    let circle_size = sizing.icon_size + sizing.padding * 2.;
    let overall_size = sizing.overall_size_override.unwrap_or(circle_size);
    let mut stack = Stack::new().with_child(
        ConstrainedBox::new(circle)
            .with_width(overall_size)
            .with_height(overall_size)
            .finish(),
    );
    stack.add_positioned_child(
        badge_with_ring,
        OffsetPositioning::offset_from_parent(
            vec2f(sizing.badge_offset.0, sizing.badge_offset.1),
            ParentOffsetBounds::ParentBySize,
            ParentAnchor::BottomRight,
            ChildAnchor::BottomRight,
        ),
    );
    ConstrainedBox::new(stack.finish())
        .with_width(overall_size)
        .with_height(overall_size)
        .finish()
}

// ---------------------------------------------------------------------------
// Ratio-based status-badge overlay for a caller-supplied avatar element.
//
// The functions above use `IconWithStatusSizing` (explicit pixel sizes) and
// only build the circle+icon themselves. The orchestration pill bar (#304
// Step 2) needs to wrap an *already-rendered* avatar (from
// `agent_view::avatar_disc::render_avatar_disc`) with the same cutout-ring
// status badge, sized proportionally from one `total_size` the way the pin's
// `render_icon_with_status_with_badge_style` does.
//
// Rather than growing the shared `IconWithStatusVariant` enum with a
// `CustomAvatar` case (which `render_icon_with_status`'s match above would
// then have to handle too, for the two existing non-pill-bar callers in
// `workspace/view/vertical_tabs.rs` and `notifications/item_rendering.rs`
// that have no use for it), this is a narrower, additive function scoped to
// exactly what the pill bar needs: wrap a pre-rendered avatar. It does not
// touch or share code with the sizing-based system above.
//
// The pin's equivalent also handles `is_ambient` (cloud) runs via a
// separate cloud-lobe badge (`render_with_cloud_status_badge`,
// `StatusColorStyle::Cloud`). Not ported: `is_remote_child` -- the flag
// that would drive `is_ambient` here -- is permanently `false` in this
// fork (see `AIConversation::is_remote_child`'s doc comment and
// TODO.md's Tier 3.5 section); there is no remote-worker execution path,
// so the cloud-lobe branch can never actually render. Callers still pass
// an `is_remote_child` bool for structural parity with the pin's
// `PillSpec`, but this function always renders the standard cutout-ring
// badge.

/// Brand-circle diameter as a fraction of `total_size`. `pub(crate)` so
/// callers that pre-render their own avatar (e.g. the pill bar) can size it
/// consistently with [`render_custom_avatar_with_status_badge`].
pub(crate) const CIRCLE_RATIO: f32 = 0.76;
const DEFAULT_BADGE_RATIO: f32 = 0.57;
const DEFAULT_BADGE_ICON_RATIO: f32 = 0.34;

/// Status-badge geometry override. Pass [`StatusBadgeStyle::DEFAULT`] for the
/// standard look.
#[derive(Clone, Copy)]
pub(crate) struct StatusBadgeStyle {
    /// Cutout-ring diameter as a fraction of `total_size`.
    pub ring_ratio: f32,
    /// Status-icon glyph diameter as a fraction of `total_size`.
    pub icon_ratio: f32,
    pub inner_shape: BadgeInnerShape,
}

#[derive(Clone, Copy)]
pub(crate) enum BadgeInnerShape {
    Circle,
    RoundedSquare { radius_px: f32 },
}

impl StatusBadgeStyle {
    pub(crate) const DEFAULT: Self = Self {
        ring_ratio: DEFAULT_BADGE_RATIO,
        icon_ratio: DEFAULT_BADGE_ICON_RATIO,
        inner_shape: BadgeInnerShape::Circle,
    };
}

/// Returns the brand-circle diameter for a given `total_size`. Callers that
/// pre-render their own avatar (to pass into
/// [`render_custom_avatar_with_status_badge`]) should size it to this so the
/// badge's overhang matches the sizing-based variants above.
pub(crate) fn circle_size(total: f32) -> f32 {
    total * CIRCLE_RATIO
}

fn badge_size(total: f32, style: StatusBadgeStyle) -> f32 {
    total * style.ring_ratio
}

fn badge_icon_size(total: f32, style: StatusBadgeStyle) -> f32 {
    total * style.icon_ratio
}

fn badge_padding(total: f32, style: StatusBadgeStyle) -> f32 {
    (badge_size(total, style) - badge_icon_size(total, style)) / 4.
}

/// Default overhang of the badge's BR past the circle's BR edge (toward the
/// box's BR), as a fraction of `total_size`. Baked into
/// `corner_overlay_offset` so most callers can just pass `0.0` for their
/// `overlay_extra_overhang_ratio`.
const DEFAULT_OVERLAY_OVERHANG_PAST_CIRCLE_EDGE: f32 = 0.19;

/// Returns the pixel offset applied to the badge's `BottomRight -> BottomRight`
/// anchor. Negative whenever the badge sits up-and-left of the box's BR
/// (the only case rendered here).
///
/// `overlay_extra_overhang_ratio` is a signed fraction of `total` added to
/// `DEFAULT_OVERLAY_OVERHANG_PAST_CIRCLE_EDGE`: `0.0` for the default
/// position most callers want; positive pushes the badge further toward the
/// box's BR; negative pulls it inward toward the circle's center.
fn corner_overlay_offset(total: f32, overlay_extra_overhang_ratio: f32) -> f32 {
    let total_overhang = DEFAULT_OVERLAY_OVERHANG_PAST_CIRCLE_EDGE + overlay_extra_overhang_ratio;
    -((1.0 - CIRCLE_RATIO) - total_overhang) * total
}

/// Wraps a pre-rendered avatar (sized to `circle_size(total_size)` by the
/// caller) with an optional status-badge cutout-ring overlay, both derived
/// proportionally from `total_size`.
///
/// `is_remote_child` is accepted for structural parity with the pin's
/// `PillSpec` but is always effectively `false` in this fork -- see the
/// module-level comment above this section.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_custom_avatar_with_status_badge(
    avatar: Box<dyn Element>,
    status: Option<&ConversationStatus>,
    is_remote_child: bool,
    total_size: f32,
    overlay_extra_overhang_ratio: f32,
    badge_style: StatusBadgeStyle,
    theme: &WarpTheme,
    status_container_background: WarpThemeFill,
) -> Box<dyn Element> {
    if is_remote_child {
        log::warn!(
            "render_custom_avatar_with_status_badge: is_remote_child was true, but this fork \
             has no remote-worker execution path (is_remote_child is permanently false); \
             rendering the standard badge anyway."
        );
    }
    let Some(status) = status else {
        return ConstrainedBox::new(avatar)
            .with_width(total_size)
            .with_height(total_size)
            .finish();
    };
    let (icon, color) = status.status_icon_and_color(theme);
    let badge_icon_diameter = badge_icon_size(total_size, badge_style);
    let pad = badge_padding(total_size, badge_style);
    let badge_icon = ConstrainedBox::new(icon.to_warpui_icon(WarpThemeFill::Solid(color)).finish())
        .with_width(badge_icon_diameter)
        .with_height(badge_icon_diameter)
        .finish();
    let inner_radius = match badge_style.inner_shape {
        BadgeInnerShape::Circle => Radius::Percentage(50.),
        BadgeInnerShape::RoundedSquare { radius_px } => Radius::Pixels(radius_px),
    };
    let badge = Container::new(badge_icon)
        .with_uniform_padding(pad)
        .with_corner_radius(CornerRadius::with_all(inner_radius))
        .finish();
    // Cutout ring around the badge; always circular (only the inner holder varies).
    let badge_with_ring = Container::new(badge)
        .with_uniform_padding(pad)
        .with_background(status_container_background)
        .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
        .finish();

    let badge_corner_offset = corner_overlay_offset(total_size, overlay_extra_overhang_ratio);
    let mut stack = Stack::new().with_child(
        ConstrainedBox::new(avatar)
            .with_width(total_size)
            .with_height(total_size)
            .finish(),
    );
    stack.add_positioned_child(
        badge_with_ring,
        OffsetPositioning::offset_from_parent(
            vec2f(badge_corner_offset, badge_corner_offset),
            ParentOffsetBounds::Unbounded,
            ParentAnchor::BottomRight,
            ChildAnchor::BottomRight,
        ),
    );
    ConstrainedBox::new(stack.finish())
        .with_width(total_size)
        .with_height(total_size)
        .finish()
}

#[cfg(test)]
#[path = "icon_with_status_tests.rs"]
mod tests;
