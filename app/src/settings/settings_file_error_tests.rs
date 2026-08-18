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
    // settings-page footer both render.
    let (heading, description) =
        SettingsFileError::InvalidSettings(vec!["Theme".to_owned()]).heading_and_description();

    assert_eq!(heading, "Your settings file contains an error.");
    assert_eq!(
        description,
        "Invalid value for 'Theme'. The default value is being used."
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
