//! `themes/phosphor_{amber,green}.yaml` are distributable copies of the
//! `phosphor_{amber,green}()` built-in themes below — hand-synced duplicates,
//! not generated from one another (the YAML files are what a user installs
//! manually; the Rust consts are what ships built in). These tests catch the
//! two drifting: if you change one, change the other and re-run this.

use super::*;

/// Asserts a YAML theme file (as distributed under `themes/`) round-trips to
/// exactly the built-in `WarpTheme` value it's meant to mirror.
fn assert_yaml_matches_builtin(yaml: &str, builtin: &WarpTheme) {
    let from_yaml: WarpTheme =
        serde_yaml::from_str(yaml).expect("themes/*.yaml must parse as a WarpTheme");
    assert_eq!(
        &from_yaml, builtin,
        "themes/*.yaml has drifted from the bundled Rust theme const; keep them hand-synced"
    );
}

#[test]
fn phosphor_amber_yaml_matches_builtin_theme() {
    let yaml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../themes/phosphor_amber.yaml"
    ));
    assert_yaml_matches_builtin(yaml, &phosphor_amber());
}

#[test]
fn phosphor_green_yaml_matches_builtin_theme() {
    let yaml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../themes/phosphor_green.yaml"
    ));
    assert_yaml_matches_builtin(yaml, &phosphor_green());
}
