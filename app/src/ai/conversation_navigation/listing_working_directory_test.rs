use super::listing_working_directory;

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

#[test]
fn initial_directory_wins_over_pane_cwd() {
    let dir = listing_working_directory(s("/proj"), s("/proj/sub"), true, || s("/tmp"));
    assert_eq!(dir, s("/proj"));
}

#[test]
fn falls_back_to_conversations_own_last_cwd_not_pane_cwd() {
    let dir = listing_working_directory(None, s("/proj/sub"), false, || s("/tmp"));
    assert_eq!(dir, s("/proj/sub"));
}

#[test]
fn unselected_conversation_never_uses_pane_cwd() {
    assert_eq!(
        listing_working_directory(None, None, false, || s("/tmp")),
        None
    );
}

#[test]
fn selected_conversation_without_any_directory_uses_pane_cwd() {
    assert_eq!(
        listing_working_directory(None, None, true, || s("/tmp")),
        s("/tmp")
    );
}
