use std::collections::BTreeSet;

use super::unknown_settings_file_keys;

fn known(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|p| (*p).to_owned()).collect()
}

#[test]
fn recognized_keys_are_not_reported() {
    let file = r#"
[appearance.cursor]
cursor_blink = true

[text_editing]
vim_mode_enabled = false
"#;
    let unknown = unknown_settings_file_keys(
        file,
        &known(&[
            "appearance.cursor.cursor_blink",
            "text_editing.vim_mode_enabled",
        ]),
    );
    assert!(unknown.is_empty(), "unexpected findings: {unknown:?}");
}

#[test]
fn a_typo_inside_a_known_section_reports_the_full_path() {
    // The point of descending into known sections: blame the leaf the user
    // mistyped, not the `[appearance.cursor]` table that is otherwise fine.
    let file = r#"
[appearance.cursor]
cursor_blink = true
cursor_blnik = false
"#;
    let unknown = unknown_settings_file_keys(file, &known(&["appearance.cursor.cursor_blink"]));
    assert_eq!(unknown, vec!["appearance.cursor.cursor_blnik".to_owned()]);
}

#[test]
fn an_unknown_top_level_key_is_reported() {
    let file = r#"
telemetry_enabled = true
"#;
    let unknown = unknown_settings_file_keys(file, &known(&["privacy.telemetry_enabled"]));
    assert_eq!(unknown, vec!["telemetry_enabled".to_owned()]);
}

#[test]
fn an_unknown_section_is_reported_once_not_per_child() {
    // A whole subsystem this fork dropped is the migrating user's most likely
    // case. Naming the section once is the useful report; naming every leaf
    // under it is noise that buries the typo in the next test.
    let file = r#"
[cloud_sync]
enabled = true
interval_seconds = 30

[cloud_sync.credentials]
token = "abc"
"#;
    let unknown = unknown_settings_file_keys(file, &known(&["privacy.telemetry_enabled"]));
    assert_eq!(unknown, vec!["cloud_sync".to_owned()]);
}

#[test]
fn the_inner_shape_of_a_structured_setting_is_not_walked() {
    // `custom_secret_regex_list`-shaped settings deserialize a table of their
    // own. Those inner keys belong to the setting's deserializer, and an
    // invalid one is reported as `SettingsFileError::InvalidSettings`, not here.
    let file = r#"
[privacy.custom_secret_regex_list]
whatever_the_user_named_it = "AKIA[0-9A-Z]{16}"
"#;
    let unknown = unknown_settings_file_keys(file, &known(&["privacy.custom_secret_regex_list"]));
    assert!(unknown.is_empty(), "unexpected findings: {unknown:?}");
}

#[test]
fn a_scalar_written_where_a_section_belongs_is_reported() {
    // `text_editing = 3` is as inert as a misspelled key, and just as silent.
    let file = "text_editing = 3\n";
    let unknown = unknown_settings_file_keys(file, &known(&["text_editing.vim_mode_enabled"]));
    assert_eq!(unknown, vec!["text_editing".to_owned()]);
}

#[test]
fn a_file_that_does_not_parse_yields_nothing() {
    // Already reported as `SettingsFileError::FileParseFailed`; a second,
    // vaguer complaint about the same file helps nobody.
    let unknown = unknown_settings_file_keys("this is not = = toml", &known(&["a.b"]));
    assert!(unknown.is_empty(), "unexpected findings: {unknown:?}");
}

#[test]
fn an_empty_file_yields_nothing() {
    assert!(unknown_settings_file_keys("", &known(&["a.b"])).is_empty());
}

#[test]
fn findings_are_sorted_so_the_report_is_stable() {
    let file = r#"
zeta = 1
alpha = 2

[middle]
key = 3
"#;
    let unknown = unknown_settings_file_keys(file, &known(&["something.else"]));
    assert_eq!(
        unknown,
        vec!["alpha".to_owned(), "middle".to_owned(), "zeta".to_owned()]
    );
}
