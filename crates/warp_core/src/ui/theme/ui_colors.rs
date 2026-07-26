//! UI color override mapping struct.
//! Loaded from a separate UI theme file, providing optional UI color overrides.
//! All fields are Option; when unset, they fall back to WarpTheme's
//! programmatically derived values.

use serde::{Deserialize, Serialize};
use warpui::color::ColorU;

use crate::ui::color::hex_color_alpha;

/// UI color override mapping. All fields are optional; when missing, uses
/// WarpTheme's default derived values.
#[derive(Serialize, Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct UiColors {
    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub surface_1: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub surface_2: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub surface_3: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub border: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub focus_border: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub split_pane_border: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub main_text: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub sub_text: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub hint_text: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub disabled_text: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub selection: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub text_selection: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub hover: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub active: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub warning: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub error: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub success: Option<ColorU>,

    #[serde(default, with = "hex_color_alpha::option", skip_serializing_if = "Option::is_none")]
    pub link: Option<ColorU>,
}

#[cfg(test)]
#[path = "ui_colors_tests.rs"]
mod tests;
