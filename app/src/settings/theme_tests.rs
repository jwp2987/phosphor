use std::path::PathBuf;

use super::*;
use crate::themes::theme::CustomTheme;
use crate::user_config;

// NOTE: Warp's upstream `theme_tests.rs` also covers `SystemThemes::current_value_is_syncable`
// (`selected_system_themes_sync_when_custom_paths_are_under_theme_root`,
// `selected_system_themes_do_not_sync_when_any_custom_path_is_outside_theme_root`) and a combined
// `built_in_theme_settings_remain_syncable` test that touches both `Theme` and `SystemThemes`.
// Those are omitted here: the fork's `SystemThemes` has no `current_value_is_syncable` method at
// all (this settings group never gained the theme-root-aware sync check that `Theme` has), so
// those tests do not compile against the current product code. This is a product gap, not
// something fixed here.

fn custom(path: PathBuf) -> ThemeKind {
    ThemeKind::Custom(CustomTheme::new("Custom".to_string(), path))
}

fn custom_base16(path: PathBuf) -> ThemeKind {
    ThemeKind::CustomBase16(CustomTheme::new("Base16 Custom".to_string(), path))
}

#[test]
fn theme_kind_syncs_custom_theme_under_theme_root() {
    let setting = Theme::new(Some(custom(user_config::themes_dir().join("custom.yml"))));

    assert!(setting.current_value_is_syncable());
}

#[test]
fn theme_kind_does_not_sync_custom_theme_outside_theme_root() {
    let setting = Theme::new(Some(custom(std::env::temp_dir().join("custom.yml"))));

    assert!(!setting.current_value_is_syncable());
}

#[test]
fn theme_kind_syncs_custom_base16_theme_under_theme_root() {
    let setting = Theme::new(Some(custom_base16(
        user_config::themes_dir().join("base16/custom.yml"),
    )));

    assert!(setting.current_value_is_syncable());
}
