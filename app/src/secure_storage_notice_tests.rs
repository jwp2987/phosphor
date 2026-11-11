use super::OncePerLaunch;

/// #2095: nothing is shown while every stored value reads; the first unreadable store is one
/// notice; every later one in the same launch is none.
#[test]
fn an_unreadable_store_shows_one_notice_per_launch() {
    let mut gate = OncePerLaunch::default();
    assert!(!gate.due(), "nothing to show while stored values read");

    gate.observe_unreadable();
    gate.observe_unreadable();
    assert!(gate.due());
    assert!(gate.confirm_shown());
    assert!(!gate.due(), "shown once");
    assert!(
        !gate.confirm_shown(),
        "a second workspace showing it is not a second notice"
    );

    // More unreadable reads later in the run (other keys, the same key again).
    gate.observe_unreadable();
    assert!(!gate.due(), "once per launch");
    assert!(!gate.confirm_shown());
}

/// A request is not a notice: seen before any window exists (the secure-storage worker reads
/// during start-up), it stays due until a workspace confirms it displayed the toast.
#[test]
fn the_notice_stays_due_until_a_workspace_displayed_it() {
    let mut gate = OncePerLaunch::default();
    gate.observe_unreadable();
    assert!(gate.due());
    // Asked a window with no workspace; nothing confirmed.
    assert!(gate.due(), "still owed to the user");
    assert!(gate.confirm_shown());
    assert!(!gate.due());
}

/// The notice is a fixed sentence: it names no key, no file and no value, in any language.
#[test]
fn the_notice_text_carries_no_secret_and_no_path() {
    for (lang, catalogue) in [
        ("en", include_str!("../i18n/en/warp.ftl")),
        ("ja", include_str!("../i18n/ja/warp.ftl")),
        ("zh-CN", include_str!("../i18n/zh-CN/warp.ftl")),
    ] {
        let line = catalogue
            .lines()
            .find(|line| line.starts_with("common-secure-storage-unreadable = "))
            .unwrap_or_else(|| panic!("{lang}: the notice string is missing"));
        assert!(
            !line.contains('{') && !line.contains('$'),
            "{lang}: the notice takes no arguments, so nothing can be interpolated into it"
        );
        assert!(
            !line.contains('/'),
            "{lang}: the notice names no path: {line}"
        );
    }
}
