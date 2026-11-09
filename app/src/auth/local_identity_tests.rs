use super::*;

#[test]
fn display_name_prefers_configured_then_os_user_then_neutral() {
    assert_eq!(resolve_display_name("Ada", Some("ada_os")), "Ada");
    assert_eq!(resolve_display_name("  ", Some("ada_os")), "ada_os");
    assert_eq!(resolve_display_name("", None), FALLBACK_DISPLAY_NAME);
    assert_eq!(resolve_display_name("", Some("  ")), FALLBACK_DISPLAY_NAME);
}

#[test]
fn display_name_is_never_the_old_stub() {
    for name in [
        resolve_display_name("", None),
        resolve_display_name("", Some("winters")),
    ] {
        assert!(!name.contains("Test User"), "{name}");
        assert!(!name.contains("warp.dev"), "{name}");
    }
}

#[test]
fn os_user_name_takes_the_first_non_blank_variable() {
    assert_eq!(
        os_user_name(Some(" "), Some("u2"), Some("u3")),
        Some("u2".into())
    );
    assert_eq!(os_user_name(None, None, Some("u3")), Some("u3".into()));
    assert_eq!(os_user_name(None, Some(""), None), None);
}

#[test]
fn default_avatar_is_stable_and_not_an_initial() {
    let a = default_avatar_for("winters");
    assert_eq!(a, default_avatar_for("Winters "));
    assert!(DEFAULT_AVATARS.contains(&a));
    assert!(a.chars().next().is_some_and(|c| !c.is_ascii()));
}

#[test]
fn empty_setting_gives_the_default_emoji() {
    let avatar = resolve_avatar("", "winters", None, |_| false);
    assert_eq!(
        avatar,
        LocalAvatar::Emoji(default_avatar_for("winters").into())
    );
}

#[test]
fn emoji_setting_is_used_and_capped() {
    assert_eq!(
        resolve_avatar("\u{1F98A}", "x", None, |_| false),
        LocalAvatar::Emoji("\u{1F98A}".into())
    );
    let long = "abcdefghijklmnop";
    match resolve_avatar(long, "x", None, |_| false) {
        LocalAvatar::Emoji(text) => assert_eq!(text.chars().count(), MAX_AVATAR_CHARS),
        other => panic!("{other:?}"),
    }
}

#[test]
fn existing_image_path_is_an_image() {
    let avatar = resolve_avatar("/tmp/me.png", "x", None, |p| p == Path::new("/tmp/me.png"));
    assert_eq!(avatar, LocalAvatar::Image(PathBuf::from("/tmp/me.png")));
}

#[test]
fn home_relative_image_path_expands() {
    let home = PathBuf::from("/home/u");
    let avatar = resolve_avatar("~/pics/me.jpg", "x", Some(&home), |p| {
        p == Path::new("/home/u/pics/me.jpg")
    });
    assert_eq!(
        avatar,
        LocalAvatar::Image(PathBuf::from("/home/u/pics/me.jpg"))
    );
}

#[test]
fn missing_image_file_falls_back_to_the_default() {
    let avatar = resolve_avatar("/nope/me.png", "winters", None, |_| false);
    assert_eq!(
        avatar,
        LocalAvatar::Emoji(default_avatar_for("winters").into())
    );
}

#[test]
fn a_bare_image_name_counts_as_a_path_not_text() {
    // "me.png" is not an emoji; with no such file it falls back rather than showing "me.png".
    let avatar = resolve_avatar("me.png", "winters", None, |_| false);
    assert_eq!(
        avatar,
        LocalAvatar::Emoji(default_avatar_for("winters").into())
    );
}
