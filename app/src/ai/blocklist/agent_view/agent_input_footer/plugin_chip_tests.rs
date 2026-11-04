use super::{PluginChipKind, installed_plugin_chip, plugin_source_move_chip_key};

#[test]
fn a_due_version_update_takes_the_chip() {
    assert_eq!(
        installed_plugin_chip(true, false, false, || true),
        Some(PluginChipKind::Update)
    );
}

/// The review's B6: dismissing the move must never hide a version update.
#[test]
fn a_dismissed_move_does_not_hide_a_version_update() {
    assert_eq!(
        installed_plugin_chip(true, false, true, || true),
        Some(PluginChipKind::Update)
    );
}

/// And the other way round: a version update dismissed at the current
/// minimum does not dismiss the move.
#[test]
fn a_dismissed_version_update_does_not_hide_the_move() {
    assert_eq!(
        installed_plugin_chip(false, true, false, || true),
        Some(PluginChipKind::Move)
    );
}

#[test]
fn a_dismissed_version_update_hides_only_itself() {
    assert_eq!(installed_plugin_chip(true, true, false, || true), None);
}

#[test]
fn a_dismissed_move_hides_only_itself() {
    assert_eq!(installed_plugin_chip(false, false, true, || true), None);
}

#[test]
fn nothing_due_shows_nothing() {
    assert_eq!(installed_plugin_chip(false, false, false, || false), None);
}

#[test]
fn the_move_check_is_skipped_when_it_cannot_matter() {
    // A due version update or a dismissed move never asks for the (config
    // reading) move check.
    assert_eq!(
        installed_plugin_chip(true, false, false, || panic!("asked")),
        Some(PluginChipKind::Update)
    );
    assert_eq!(
        installed_plugin_chip(false, false, true, || panic!("asked")),
        None
    );
}

#[test]
fn the_move_dismissal_key_is_separate_from_the_version_key() {
    assert_eq!(plugin_source_move_chip_key("claude"), "claude#source-move");
    assert_eq!(
        plugin_source_move_chip_key("claude@host"),
        "claude@host#source-move"
    );
    assert_ne!(plugin_source_move_chip_key("claude"), "claude");
}
