use super::*;

#[test]
fn unused_theme_name_keeps_a_free_name() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(unused_theme_name(dir.path(), "sunset", "png"), "sunset");
}

/// #873: a hand-written theme with the same name must not be replaced.
#[test]
fn unused_theme_name_skips_an_existing_yaml() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sunset.yaml"), "mine").unwrap();
    assert_eq!(unused_theme_name(dir.path(), "sunset", "png"), "sunset (2)");

    std::fs::write(dir.path().join("sunset (2).yaml"), "also mine").unwrap();
    assert_eq!(unused_theme_name(dir.path(), "sunset", "png"), "sunset (3)");
}

/// An existing image of the same name counts too: copying over it would
/// replace someone's file.
#[test]
fn unused_theme_name_skips_an_existing_image() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sunset.png"), "image").unwrap();
    assert_eq!(unused_theme_name(dir.path(), "sunset", "png"), "sunset (2)");
    // A different extension does not collide.
    assert_eq!(unused_theme_name(dir.path(), "sunset", "jpg"), "sunset");
}

#[cfg(feature = "local_fs")]
#[test]
fn is_same_file_detects_the_same_file_through_different_paths() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.png");
    std::fs::write(&file, "image").unwrap();
    let roundabout = dir.path().join(".").join("a.png");
    assert!(is_same_file(&file, &roundabout));
    assert!(!is_same_file(&file, &dir.path().join("missing.png")));
}
