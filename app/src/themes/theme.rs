use super::default_themes::*;
use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::iter::FromIterator;
use std::path::{Component, Path, PathBuf};
use warp_core::ui::color::pick_foreground_color;
use warpui::assets::asset_cache::AssetSource;
use warpui::{
    color::ColorU,
    elements::{
        Align, Border, ConstrainedBox, Container, Element, Empty, Flex, ParentElement, Rect,
        Shrinkable, Stack, Text,
    },
    fonts::FamilyId,
};

use super::theme_creator::{pick_accent_color_from_options, top_colors_for_image};

pub use warp_core::ui::color::blend::Blend;
pub use warp_core::ui::theme::*;

const THUMBNAIL_MARGIN: f32 = 10.;

// We use the discriminant of enum variants to determine the order of theme types in the
// theme chooser view.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    Hash,
    Eq,
    Ord,
    PartialOrd,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(description = "The color theme.", rename_all = "snake_case")]
pub enum ThemeKind {
    // Need an alias for backwards-compatibility: Originally we only had a single reward theme
    // so it was named `ReferralReward`.
    #[serde(alias = "ReferralReward")]
    #[schemars(skip)]
    SentReferralReward,
    #[schemars(skip)]
    ReceivedReferralReward,
    #[schemars(description = "Adeberry")]
    Adeberry,
    #[schemars(description = "Phenomenon")]
    Phenomenon,
    #[schemars(description = "Dark")]
    Dark,
    #[schemars(description = "Dracula")]
    Dracula,
    #[schemars(description = "Tokyo Night")]
    TokyoNight,
    #[schemars(description = "One Dark")]
    OneDark,
    #[default]
    #[schemars(description = "Phosphor Amber")]
    PhosphorAmber,
    #[schemars(description = "Phosphor Green")]
    PhosphorGreen,
    #[schemars(description = "Fancy Dracula")]
    FancyDracula,
    #[schemars(description = "Cyber Wave")]
    CyberWave,
    #[schemars(description = "Solar Flare")]
    SolarFlare,
    #[schemars(description = "Solarized Dark")]
    SolarizedDark,
    #[schemars(description = "Willow Dream")]
    WillowDream,
    #[schemars(description = "Light")]
    Light,
    #[schemars(description = "Dark City")]
    DarkCity,
    #[schemars(description = "Gruvbox Dark")]
    GruvboxDark,
    #[schemars(description = "Red Rock")]
    RedRock,
    #[schemars(description = "Jellyfish")]
    JellyFish,
    #[schemars(description = "Leafy")]
    Leafy,
    #[schemars(description = "WezTerm Classic")]
    WezTermClassic,
    #[schemars(description = "VS Code 2026 Dark")]
    VsCode2026Dark,
    #[schemars(description = "Koi")]
    Koi,
    #[schemars(description = "Solarized Light")]
    SolarizedLight,
    #[schemars(description = "Snowy")]
    Snowy,
    #[schemars(description = "Gruvbox Light")]
    GruvboxLight,
    #[schemars(description = "Pink City")]
    PinkCity,
    #[schemars(description = "Marble")]
    Marble,
    #[schemars(description = "A user-provided custom theme loaded from a file.")]
    Custom(CustomTheme),
    /// Base16 themes are a special case of custom themes with their own semantics for ANSI colors that override "bright" color variants.
    #[schemars(description = "A custom theme using the Base16 color scheme format.")]
    CustomBase16(CustomTheme),
    #[schemars(skip)]
    InMemory(InMemoryThemeOptions),
}

impl From<CustomTheme> for ThemeKind {
    fn from(custom_theme: CustomTheme) -> ThemeKind {
        if custom_theme.name.as_str().starts_with("Base16") {
            ThemeKind::CustomBase16(custom_theme)
        } else {
            ThemeKind::Custom(custom_theme)
        }
    }
}

impl std::fmt::Display for ThemeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match &self {
            ThemeKind::Light => "Light",
            ThemeKind::Dark => "Dark",
            ThemeKind::Dracula => "Dracula",
            ThemeKind::TokyoNight => "Tokyo Night",
            ThemeKind::OneDark => "One Dark",
            ThemeKind::PhosphorAmber => "Phosphor Amber",
            ThemeKind::PhosphorGreen => "Phosphor Green",
            ThemeKind::SolarizedDark => "Solarized Dark",
            ThemeKind::SolarizedLight => "Solarized Light",
            ThemeKind::GruvboxDark => "Gruvbox Dark",
            ThemeKind::GruvboxLight => "Gruvbox Light",
            ThemeKind::JellyFish => "Jellyfish",
            ThemeKind::Koi => "Koi",
            ThemeKind::Leafy => "Leafy",
            ThemeKind::Marble => "Marble",
            ThemeKind::PinkCity => "Pink City",
            ThemeKind::Snowy => "Snowy",
            ThemeKind::DarkCity => "Dark City",
            ThemeKind::RedRock => "Red Rock",
            ThemeKind::CyberWave => "Cyber Wave",
            ThemeKind::WillowDream => "Willow Dream",
            ThemeKind::FancyDracula => "Fancy Dracula",
            ThemeKind::Phenomenon => "Phenomenon",
            ThemeKind::SolarFlare => "Solar Flare",
            ThemeKind::Adeberry => "Adeberry",
            ThemeKind::WezTermClassic => "WezTerm Classic",
            ThemeKind::VsCode2026Dark => "VS Code 2026 Dark",
            ThemeKind::SentReferralReward => "Phosphor Referral",
            ThemeKind::ReceivedReferralReward => "Referred to Phosphor",
            ThemeKind::Custom(custom_theme) => custom_theme.name.as_str(),
            ThemeKind::CustomBase16(custom_theme) => custom_theme.name.as_str(),
            ThemeKind::InMemory(in_memory_theme) => in_memory_theme.name.as_str(),
        };
        write!(f, "{value}")
    }
}

impl ThemeKind {
    pub fn matches(&self, query: &str) -> bool {
        let theme_name = format!("{self}").to_lowercase();
        theme_name.contains(&query.to_lowercase())
    }

    /// Whether this theme kind can round-trip through `settings.toml` on a
    /// different machine/OS/username. Built-in themes always can; a custom
    /// theme can only if its file path is stored as a path relative to the
    /// themes directory (see `custom_theme_path_is_portable`).
    pub(crate) fn is_custom_theme_reference_syncable(&self) -> bool {
        match self {
            ThemeKind::Custom(custom_theme) | ThemeKind::CustomBase16(custom_theme) => {
                custom_theme_path_is_portable(&custom_theme.path, &crate::user_config::themes_dir())
            }
            _ => true,
        }
    }
}

#[derive(
    Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, schemars::JsonSchema,
)]
#[schemars(description = "A user-provided custom theme.")]
pub struct CustomTheme {
    #[schemars(description = "The display name of the custom theme.")]
    name: String,
    #[serde(
        deserialize_with = "deserialize_custom_theme_path",
        serialize_with = "serialize_custom_theme_path"
    )]
    #[schemars(description = "The file path to the custom theme definition.")]
    path: PathBuf,
}

// Custom themes store their file path in `settings.toml` as a path relative to the themes
// directory when the file lives under it ("portable"), instead of an absolute,
// machine-specific path. This lets a `custom: { path: "catppuccin/mocha.yml" }` entry resolve
// correctly on a different machine/OS/username. Paths that don't live under the themes
// directory (or that use a foreign OS's absolute-path syntax) are preserved as-is: they can't
// be made portable, so they're kept literally rather than silently rewritten.
impl settings_value::SettingsValue for CustomTheme {
    fn to_file_value(&self) -> serde_json::Value {
        serde_json::json!({
            "name": &self.name,
            "path": custom_theme_path_storage_value(&self.path, &crate::user_config::themes_dir()),
        })
    }

    fn from_file_value(value: &serde_json::Value) -> Option<Self> {
        #[derive(Deserialize)]
        struct FileValue {
            name: String,
            path: String,
        }

        let value = serde_json::from_value::<FileValue>(value.clone()).ok()?;
        Some(Self {
            name: value.name,
            path: portable_custom_theme_path_from_stored_raw(
                &value.path,
                &crate::user_config::themes_dir(),
            ),
        })
    }
}

fn serialize_custom_theme_path<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(path) =
        portable_custom_theme_storage_string(path, &crate::user_config::themes_dir())
    {
        path.serialize(serializer)
    } else {
        path.serialize(serializer)
    }
}

fn deserialize_custom_theme_path<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;
    Ok(portable_custom_theme_path_from_stored_raw(
        &path,
        &crate::user_config::themes_dir(),
    ))
}

fn custom_theme_path_storage_value(path: &Path, theme_root: &Path) -> serde_json::Value {
    if let Some(path) = portable_custom_theme_storage_string(path, theme_root) {
        serde_json::Value::String(path)
    } else {
        serde_json::json!(path)
    }
}

/// Whether `path` can round-trip through portable storage relative to `theme_root`: either it
/// is already a portable relative raw string, or it is an absolute (native or foreign-OS)
/// path that resolves under `theme_root`.
pub(crate) fn custom_theme_path_is_portable(path: &Path, theme_root: &Path) -> bool {
    if path_is_absolute_or_foreign_absolute(path) {
        return portable_custom_theme_storage_string(path, theme_root).is_some();
    }

    path.to_str()
        .is_some_and(|path| portable_stored_raw_components(path).is_some())
}

/// Resolves a raw stored path string (as read from `settings.toml`) into an absolute path.
/// A portable relative raw string resolves under `theme_root`; anything else (legacy absolute
/// paths, foreign-OS paths, unportable relative forms) is preserved verbatim.
pub(crate) fn portable_custom_theme_path_from_stored_raw(raw: &str, theme_root: &Path) -> PathBuf {
    portable_stored_raw_components(raw)
        .map(|components| {
            components
                .iter()
                .fold(theme_root.to_path_buf(), |path, component| {
                    path.join(component)
                })
        })
        .unwrap_or_else(|| PathBuf::from(raw))
}

/// Converts an absolute `path` into a portable storage string relative to `theme_root`, using
/// `/` as the separator regardless of host OS (so the stored form is OS-independent), or
/// `None` if `path` doesn't resolve under `theme_root` (or contains components that can't
/// round-trip, like `..` or a literal backslash).
pub(crate) fn portable_custom_theme_storage_string(
    path: &Path,
    theme_root: &Path,
) -> Option<String> {
    if path_starts_with_windows_drive_prefix_using_forward_slash(path) {
        return None;
    }

    let relative = path.strip_prefix(theme_root).ok()?;
    let mut components = Vec::new();

    for component in relative.components() {
        let Component::Normal(value) = component else {
            return None;
        };
        let value = value.to_str()?;
        if value.contains('\\') {
            return None;
        }
        components.push(value);
    }

    if components.is_empty() {
        return None;
    }

    let path = components.join("/");
    if portable_stored_raw_components(&path).is_some() {
        Some(path)
    } else {
        None
    }
}

/// Splits a raw stored path string into portable components (`/`-separated, no leading `/`,
/// no `.`/`..`, no backslash, no Windows drive prefix), or `None` if the raw string isn't a
/// portable relative form.
fn portable_stored_raw_components(raw: &str) -> Option<Vec<&str>> {
    if raw.is_empty()
        || raw.contains('\\')
        || raw.starts_with('/')
        || raw_starts_with_windows_drive_prefix(raw)
    {
        return None;
    }

    let components = raw.split('/').collect::<Vec<_>>();
    if components
        .iter()
        .all(|component| !component.is_empty() && *component != "." && *component != "..")
    {
        Some(components)
    } else {
        None
    }
}

fn raw_starts_with_windows_drive_prefix(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn path_starts_with_windows_drive_prefix_using_forward_slash(path: &Path) -> bool {
    let Some(path) = path.as_os_str().to_str() else {
        return false;
    };

    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

/// Whether `path` is absolute on this host OS, or *looks* like an absolute path from a
/// different OS (e.g. a Windows drive-letter or UNC path parsed on Unix, where it doesn't
/// have a root by Unix's own rules). Such paths are never portable, but they must still be
/// recognized so their raw form is preserved instead of being misread as relative.
fn path_is_absolute_or_foreign_absolute(path: &Path) -> bool {
    path.has_root() || path_looks_like_foreign_windows_absolute(path)
}

fn path_looks_like_foreign_windows_absolute(path: &Path) -> bool {
    if path.has_root() {
        return false;
    }

    let Some(path) = path.as_os_str().to_str() else {
        return false;
    };

    let bytes = path.as_bytes();
    let starts_with_drive_root = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');

    starts_with_drive_root || path.starts_with(r"\\")
}

impl CustomTheme {
    pub fn new(s: String, p: PathBuf) -> Self {
        CustomTheme { name: s, path: p }
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

#[derive(
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    settings_value::SettingsValue,
)]
pub struct InMemoryThemeOptions {
    name: String,
    path: PathBuf,
    #[serde(skip)]
    possible_bg_colors: Vec<ColorU>,
    #[serde(skip)]
    chosen_bg_color_index: usize,
}

impl InMemoryThemeOptions {
    pub async fn new(name: String, path: PathBuf) -> Result<Self> {
        top_colors_for_image(path.clone()).map(|top_colors| InMemoryThemeOptions {
            name,
            path,
            possible_bg_colors: top_colors,
            chosen_bg_color_index: 0,
        })
    }

    pub fn chosen_bg_color(&self) -> ColorU {
        self.possible_bg_colors[self.chosen_bg_color_index]
    }

    pub fn possible_bg_colors(&self) -> Vec<ColorU> {
        self.possible_bg_colors.clone()
    }

    pub fn chosen_bg_color_index(&self) -> usize {
        self.chosen_bg_color_index
    }

    pub fn set_chosen_bg_color_index(&mut self, index: usize) {
        self.chosen_bg_color_index = index;
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    pub fn theme(&self) -> WarpTheme {
        let bg_color = self.chosen_bg_color();
        let fg_color = pick_foreground_color(bg_color);
        let possible_accent_colors: Vec<ColorU> = self
            .possible_bg_colors
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.chosen_bg_color_index)
            .map(|(_, color)| *color)
            .collect();
        let accent_color =
            pick_accent_color_from_options(&[bg_color, fg_color], &possible_accent_colors[..]);

        let (details, terminal_colors) = if fg_color.eq(&ColorU::white()) {
            (Details::Darker, dark_mode_colors())
        } else {
            (Details::Lighter, light_mode_colors())
        };

        WarpTheme::new(
            bg_color.into(),
            fg_color,
            accent_color.into(),
            None,
            Some(details),
            terminal_colors,
            Some(Image {
                // Note that, as an invariant, in-memory themes come from local files.
                source: AssetSource::LocalFile {
                    path: self.path().to_str().unwrap_or_default().to_owned(),
                    content_version: None,
                },
                opacity: 30,
            }),
            Some(self.name()),
            None,
        )
    }
}

#[derive(Debug, Clone)]
pub struct WarpThemeConfig {
    theme_map: HashMap<ThemeKind, WarpTheme>,
}

impl WarpThemeConfig {
    pub fn new() -> Self {
        // preload with built-in themes
        let theme_map: HashMap<ThemeKind, WarpTheme> = HashMap::from_iter([
            (ThemeKind::SentReferralReward, sent_referral_reward()),
            (
                ThemeKind::ReceivedReferralReward,
                received_referral_reward(),
            ),
            (ThemeKind::Dark, dark_theme()),
            (ThemeKind::Light, light_theme()),
            (ThemeKind::SolarizedDark, solarized_dark()),
            (ThemeKind::SolarizedLight, solarized_light()),
            (ThemeKind::Dracula, dracula()),
            (ThemeKind::TokyoNight, tokyo_night()),
            (ThemeKind::OneDark, one_dark()),
            (ThemeKind::GruvboxDark, gruvbox_dark()),
            (ThemeKind::GruvboxLight, gruvbox_light()),
            (ThemeKind::JellyFish, jellyfish()),
            (ThemeKind::Koi, koi()),
            (ThemeKind::Leafy, leafy()),
            (ThemeKind::Marble, marble()),
            (ThemeKind::PinkCity, pink_city()),
            (ThemeKind::Snowy, snowy()),
            (ThemeKind::DarkCity, dark_city()),
            (ThemeKind::RedRock, red_rock()),
            (ThemeKind::CyberWave, cyber_wave()),
            (ThemeKind::WillowDream, willow_dream()),
            (ThemeKind::FancyDracula, fancy_dracula()),
            (ThemeKind::Phenomenon, phenomenon()),
            (ThemeKind::SolarFlare, solar_flare()),
            (ThemeKind::Adeberry, adeberry()),
            (ThemeKind::WezTermClassic, wezterm_classic()),
            (ThemeKind::VsCode2026Dark, vscode_2026_dark()),
            (ThemeKind::PhosphorAmber, phosphor_amber()),
            (ThemeKind::PhosphorGreen, phosphor_green()),
        ]);
        WarpThemeConfig { theme_map }
    }

    pub fn add_new_theme(&mut self, theme_name: ThemeKind, theme: WarpTheme) {
        self.theme_map.insert(theme_name, theme);
    }

    pub fn file_to_theme(name: String, path: PathBuf) -> ThemeKind {
        CustomTheme::new(name, path).into()
    }

    pub fn theme_items(&self) -> impl Iterator<Item = (&ThemeKind, &WarpTheme)> {
        self.theme_map.iter()
    }

    pub fn theme(&self, name: &ThemeKind) -> WarpTheme {
        self.theme_map.get(name).cloned().unwrap_or_else(dark_theme)
    }
}

impl Default for WarpThemeConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RespectSystemTheme {
    #[default]
    Off,
    On(SelectedSystemThemes),
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(description = "Themes to use when following the system light/dark mode.")]
pub struct SelectedSystemThemes {
    #[schemars(description = "The theme to use in light mode.")]
    pub light: ThemeKind,
    #[schemars(description = "The theme to use in dark mode.")]
    pub dark: ThemeKind,
}

impl RespectSystemTheme {
    pub fn selected_system_themes(&self) -> Option<&SelectedSystemThemes> {
        match self {
            RespectSystemTheme::Off => None,
            RespectSystemTheme::On(selected) => Some(selected),
        }
    }
}

impl Default for SelectedSystemThemes {
    fn default() -> Self {
        Self {
            light: ThemeKind::Light,
            dark: ThemeKind::Dark,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PromptColors {
    pub input_prompt_conversation_management: ColorU,
    pub input_prompt_pwd: ColorU,
    pub input_prompt_git: ColorU,
    pub input_prompt_branch: ColorU,
    pub input_prompt_agent_mode_hint: ColorU,
    pub input_prompt_agent_mode_tasks: ColorU,
    pub input_prompt_dirty_color: ColorU,
    pub input_prompt_virtual_env: ColorU,
    pub input_prompt_user_and_host: ColorU,
    pub input_prompt_date: ColorU,
    pub input_prompt_time: ColorU,
    pub input_prompt_kubernetes: ColorU,
    pub input_prompt_svn: ColorU,
    pub input_prompt_separator: ColorU,
    pub input_prompt_subshell: ColorU,
    pub input_prompt_ssh: ColorU,
}

impl From<WarpTheme> for PromptColors {
    fn from(theme: WarpTheme) -> Self {
        PromptColors {
            input_prompt_conversation_management: theme.terminal_colors().normal.white.into(),
            input_prompt_pwd: theme.terminal_colors().normal.magenta.into(),
            input_prompt_git: theme.terminal_colors().normal.green.into(),
            input_prompt_agent_mode_hint: theme.terminal_colors().normal.yellow.into(),
            input_prompt_agent_mode_tasks: theme.terminal_colors().normal.yellow.into(),
            input_prompt_branch: theme.terminal_colors().normal.yellow.into(),
            input_prompt_dirty_color: theme.terminal_colors().normal.green.into(),
            input_prompt_virtual_env: theme.terminal_colors().normal.yellow.into(),
            input_prompt_user_and_host: theme.terminal_colors().normal.green.into(),
            input_prompt_date: theme.terminal_colors().normal.cyan.into(),
            input_prompt_time: theme.terminal_colors().normal.red.into(),
            input_prompt_kubernetes: theme.terminal_colors().normal.cyan.into(),
            input_prompt_ssh: theme.terminal_colors().normal.blue.into(),
            input_prompt_subshell: theme.terminal_colors().normal.blue.into(),
            input_prompt_svn: theme.terminal_colors().normal.blue.into(),
            input_prompt_separator: theme.terminal_colors().normal.magenta.into(),
        }
    }
}

pub fn render_preview(
    theme: &WarpTheme,
    font_family: FamilyId,
    form_factor: Option<f32>,
) -> Box<dyn Element> {
    let text_size = 8. * form_factor.unwrap_or(1.);
    let margin = THUMBNAIL_MARGIN * form_factor.unwrap_or(1.);
    let padding = 5. * form_factor.unwrap_or(1.);
    let text_line_1 = Container::new(
        Text::new_inline("ls", font_family, text_size)
            .with_color(theme.foreground().into_solid())
            .finish(),
    )
    .with_margin_left(margin)
    .with_margin_right(margin)
    .finish();

    let text_line_2 = Container::new(
        Flex::row()
            .with_child(
                Text::new_inline("dir   ", font_family, text_size)
                    .with_color(theme.terminal_colors().normal.blue.into())
                    .finish(),
            )
            .with_child(
                Text::new_inline("executable   ", font_family, text_size)
                    .with_color(theme.terminal_colors().normal.red.into())
                    .finish(),
            )
            .with_child(
                Text::new_inline("file", font_family, text_size)
                    .with_color(theme.foreground().into_solid())
                    .finish(),
            )
            .finish(),
    )
    .with_margin_left(margin)
    .with_margin_right(margin)
    .finish();

    let input_box = Shrinkable::new(
        1.,
        Align::new(
            Flex::column()
                // The border above the input box.
                .with_child(
                    Container::new(Empty::new().finish())
                        .with_padding_bottom(padding)
                        .with_border(
                            Border::top(1.2 * form_factor.unwrap_or(1.))
                                .with_border_color(theme.outline().into_solid()),
                        )
                        .finish(),
                )
                // The fake cursor within the input box.
                .with_child(
                    Container::new(
                        ConstrainedBox::new(
                            Rect::new()
                                .with_background_color(theme.accent().into_solid())
                                .finish(),
                        )
                        .with_height(12. * form_factor.unwrap_or(1.))
                        .with_width(2. * form_factor.unwrap_or(1.))
                        .finish(),
                    )
                    .with_margin_left(margin)
                    .with_margin_right(margin)
                    .finish(),
                )
                .finish(),
        )
        .bottom_left()
        .finish(),
    )
    .finish();

    let mut thumbnail = Stack::new();
    let mut background_opacity = 100;
    if let Some(background_image) = theme.background_image() {
        thumbnail.add_child(
            Shrinkable::new(
                1.,
                warpui::elements::Image::new(
                    background_image.source(),
                    warpui::elements::CacheOption::BySize,
                )
                .cover()
                .finish(),
            )
            .finish(),
        );
        background_opacity -= background_image.opacity;
    }

    thumbnail.add_child(
        Container::new(
            Container::new(
                Flex::column()
                    .with_child(text_line_1)
                    .with_child(
                        Container::new(text_line_2)
                            .with_padding_top(padding)
                            .finish(),
                    )
                    .with_child(input_box)
                    .finish(),
            )
            .with_margin_top(margin)
            .with_margin_bottom(margin)
            .finish(),
        )
        .with_background(theme.background().with_opacity(background_opacity))
        .finish(),
    );

    Align::new(
        Container::new(
            ConstrainedBox::new(thumbnail.finish())
                .with_height(100. * form_factor.unwrap_or(1.))
                .with_width(190. * form_factor.unwrap_or(1.))
                .finish(),
        )
        .finish(),
    )
    .finish()
}

#[cfg(test)]
#[path = "theme_test.rs"]
mod tests;
