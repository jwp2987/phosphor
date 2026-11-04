use std::borrow::Cow;

use warp_core::ui::{appearance::Appearance, theme::color::internal_colors};
use warpui::{
    color::ColorU,
    ui_components::{
        components::{UiComponent as _, UiComponentStyles},
        text::Span,
    },
};

/// Background color for the sharing dialog.
#[allow(dead_code)] // used by sibling unused code or shared name
pub fn dialog_background(appearance: &Appearance) -> ColorU {
    appearance.theme().surface_1().into_solid()
}

/// Text color for secondary ACL information.
#[allow(dead_code)] // used by sibling unused code or shared name
pub fn acl_secondary_text_color(appearance: &Appearance) -> ColorU {
    internal_colors::text_sub(appearance.theme(), dialog_background(appearance))
}

/// Text color for non-interactive labels.
#[allow(dead_code)] // used by warp_tui (tui feature)
pub fn label_text(appearance: &Appearance) -> ColorU {
    internal_colors::text_disabled(appearance.theme(), dialog_background(appearance))
}

/// Create a detail-text span.
#[allow(dead_code)] // used by sibling unused code or shared name
pub fn detail_text(text: impl Into<Cow<'static, str>>, appearance: &Appearance) -> Span {
    appearance
        .ui_builder()
        .span(text)
        .with_style(UiComponentStyles {
            font_color: Some(acl_secondary_text_color(appearance)),
            ..Default::default()
        })
}
