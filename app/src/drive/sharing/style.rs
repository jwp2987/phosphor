use std::borrow::Cow;

use warp_core::ui::{appearance::Appearance, theme::color::internal_colors};
use warpui::{
    color::ColorU,
    elements::{CornerRadius, Radius},
    fonts::Weight,
    ui_components::{
        components::{UiComponent as _, UiComponentStyles},
        text::Span,
    },
};

/// The gap between ACL items in the sharing dialog. Because the UI framework doesn't support gaps,
/// items should generally have vertical margins of `ACL_ITEM_GAP / 2`.
pub const ACL_ITEM_GAP: f32 = 10.;

/// The height of ACL items, not including spacing.
pub const ACL_ITEM_HEIGHT: f32 = 32.;

/// Background color for the sharing dialog.
pub fn dialog_background(appearance: &Appearance) -> ColorU {
    appearance.theme().surface_1().into_solid()
}

/// Text color for secondary ACL information.
pub fn acl_secondary_text_color(appearance: &Appearance) -> ColorU {
    internal_colors::text_sub(appearance.theme(), dialog_background(appearance))
}

/// Text color for non-interactive labels.
pub fn label_text(appearance: &Appearance) -> ColorU {
    internal_colors::text_disabled(appearance.theme(), dialog_background(appearance))
}

/// Common ACL avatar styles.
pub fn subject_avatar_styles(appearance: &Appearance) -> UiComponentStyles {
    UiComponentStyles {
        width: Some(24.),
        height: Some(24.),
        border_radius: Some(CornerRadius::with_all(Radius::Percentage(50.))),
        font_size: Some(appearance.ui_font_size()),
        font_family_id: Some(appearance.ui_font_family()),
        font_weight: Some(Weight::Bold),
        ..Default::default()
    }
}

/// Create a detail-text span.
pub fn detail_text(text: impl Into<Cow<'static, str>>, appearance: &Appearance) -> Span {
    appearance
        .ui_builder()
        .span(text)
        .with_style(UiComponentStyles {
            font_color: Some(acl_secondary_text_color(appearance)),
            ..Default::default()
        })
}
