//! User-facing copy for [`super::SettingsFileError`].
//!
//! These assert on wording, which is unusual, but the wording is the whole
//! feature: `UnknownKeys` exists precisely because reusing `InvalidSettings`
//! told the user "the default value is being used" about a setting that does
//! not exist, so nothing was defaulted and nothing was ignored in the way the
//! sentence implies. The assertions below are what stops that regressing.

use super::SettingsFileError;

#[test]
fn unknown_keys_names_the_offending_key_and_says_it_is_ignored() {
    let error = SettingsFileError::UnknownKeys(vec!["appearance.cursor.cursor_blnik".to_owned()]);
    let (heading, description) = error.heading_and_description();

    assert!(
        description.contains("appearance.cursor.cursor_blnik"),
        "the user cannot fix a key the message won't name: {description}"
    );
    assert!(
        description.contains("ignored"),
        "must say the line has no effect: {description}"
    );
    assert!(
        heading.contains("recognize"),
        "heading should say the key is unrecognized: {heading}"
    );
}

#[test]
fn unknown_keys_never_claims_a_default_was_used() {
    // The `InvalidSettings` copy this variant was carved out of. There is no
    // such setting, so there is no default to fall back to, and saying there
    // is sends the user hunting for a value they never set.
    let error = SettingsFileError::UnknownKeys(vec![
        "cloud_sync".to_owned(),
        "telemetry_enabled".to_owned(),
    ]);
    let (_, description) = error.heading_and_description();

    assert!(
        !description.contains("default"),
        "unknown keys have no default to fall back to: {description}"
    );
    assert!(
        !description.contains("Invalid value"),
        "the value isn't invalid; there is no such setting: {description}"
    );
}

#[test]
fn unknown_keys_lists_every_key_when_there_are_several() {
    let error = SettingsFileError::UnknownKeys(vec![
        "cloud_sync".to_owned(),
        "telemetry_enabled".to_owned(),
    ]);
    let (_, description) = error.heading_and_description();

    assert!(description.contains("cloud_sync"), "{description}");
    assert!(description.contains("telemetry_enabled"), "{description}");
}

#[test]
fn invalid_settings_copy_is_unchanged() {
    // Guards the split itself: adding `UnknownKeys` must not have moved the
    // pre-existing variant's wording, which the workspace banner and the
    // settings-page footer both render. The wording was changed deliberately
    // once, by #857: a rejected value is not always replaced by the default
    // (opacity 0 runs at 1, blur 200 at 64), so "the default value is being
    // used" was not always true.
    let (heading, description) =
        SettingsFileError::InvalidSettings(vec!["Theme".to_owned()]).heading_and_description();

    assert_eq!(heading, "Your settings file contains an error.");
    assert_eq!(
        description,
        "Invalid value for 'Theme'. A valid value is being used instead."
    );
}

#[test]
fn display_is_self_describing_for_the_fix_with_agent_prompt() {
    // `WorkspaceAction::FixSettingsWithOz` hands `Display` straight to the
    // agent as the entire problem statement, so it has to stand alone.
    assert_eq!(
        SettingsFileError::UnknownKeys(vec!["terminal.blinking_curser".to_owned()]).to_string(),
        "'terminal.blinking_curser' isn't a setting in this build"
    );
    assert_eq!(
        SettingsFileError::UnknownKeys(vec!["a".to_owned(), "b".to_owned()]).to_string(),
        "These aren't settings in this build: a, b"
    );
}

#[test]
fn directory_and_oversized_settings_files_are_not_called_syntax_errors() {
    use warpui_extras::user_preferences::Error;

    // #895: a directory used to show "Couldn't parse due to invalid syntax".
    let dir_error = SettingsFileError::from_load_error(&Error::NotAFile(std::path::PathBuf::from(
        "/home/u/.config/phosphor/settings.toml",
    )));
    assert!(matches!(dir_error, SettingsFileError::FileUnusable(_)));
    let (heading, description) = dir_error.heading_and_description();
    assert_eq!(heading, "Your settings file can't be used.");
    assert!(description.contains("settings.toml is not a file"));
    assert!(!description.contains("syntax"));

    // #863: too large to parse.
    let big_error = SettingsFileError::from_load_error(&Error::TooLarge {
        path: std::path::PathBuf::from("settings.toml"),
        size: 50_000_000,
        limit: 2_097_152,
    });
    assert!(big_error.to_string().contains("50000000 bytes"));

    // A real parse failure keeps its message.
    let parse_error = SettingsFileError::from_load_error(&Error::Unknown(anyhow::anyhow!("bad")));
    assert!(matches!(parse_error, SettingsFileError::FileParseFailed(_)));
}
