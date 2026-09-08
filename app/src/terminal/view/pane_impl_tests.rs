use super::{conversation_directory_to_display, resolve_conversation_directory_path};
use std::path::PathBuf;

// `conversation_directory_to_display` -- distinguishing "not a conversation pane" from "a
// conversation pane with no explicit path" is the whole point of this function; see
// `app/src/terminal/view/pane_impl.rs`'s doc comment on it.

#[test]
fn not_a_conversation_pane_displays_nothing_regardless_of_startup_path() {
    // Breaks if `is_conversation_only` stops gating the result, e.g. replacing
    // `is_conversation_only.then_some(session_startup_path)` with
    // `Some(session_startup_path)`.
    assert_eq!(conversation_directory_to_display(false, None), None);
    assert_eq!(
        conversation_directory_to_display(false, Some(PathBuf::from("/tmp"))),
        None
    );
}

#[test]
fn conversation_pane_with_no_explicit_path_is_some_none_not_none() {
    // This is the exact distinction defect 1 was about: breaks if the two levels get
    // flattened back into one, e.g. returning `session_startup_path` directly (which would
    // make this `None`, indistinguishable from "not a conversation pane").
    assert_eq!(conversation_directory_to_display(true, None), Some(None));
}

#[test]
fn conversation_pane_with_explicit_path_is_some_some_path() {
    // Breaks if the explicit path is dropped or replaced, e.g. always returning `Some(None)`.
    let path = PathBuf::from("/home/josh/git/phosphor");
    assert_eq!(
        conversation_directory_to_display(true, Some(path.clone())),
        Some(Some(path))
    );
}

// `resolve_conversation_directory_path` -- the "unset path means home" fallback itself.

#[test]
fn not_applicable_resolves_to_none_even_with_a_home_dir() {
    // Breaks if the `?` early return is removed and the outer `None` is treated as "no
    // explicit path" instead of "not a conversation pane", e.g. resolving to `home_dir`.
    assert_eq!(
        resolve_conversation_directory_path(None, Some(PathBuf::from("/home/josh"))),
        None
    );
}

#[test]
fn explicit_path_wins_over_home_dir() {
    // Breaks if the fallback is applied unconditionally, e.g. `home_dir.or(directory_to_display?)`
    // instead of `directory_to_display?.or(home_dir)`.
    let explicit = PathBuf::from("/workspace/project");
    assert_eq!(
        resolve_conversation_directory_path(
            Some(Some(explicit.clone())),
            Some(PathBuf::from("/home/josh"))
        ),
        Some(explicit)
    );
}

#[test]
fn no_explicit_path_falls_back_to_home_dir() {
    // This is defect 1 itself: breaks if the fallback is dropped, e.g.
    // `directory_to_display.flatten()` (which would resolve to `None` here, the original bug
    // -- a restored pane whose stored directory no longer exists renders nothing).
    let home = PathBuf::from("/home/josh");
    assert_eq!(
        resolve_conversation_directory_path(Some(None), Some(home.clone())),
        Some(home)
    );
}

#[test]
fn no_explicit_path_and_no_home_dir_resolves_to_none() {
    // Documents the deliberate "genuinely nothing to show" case. Breaks if this is changed to
    // synthesize a path (e.g. an empty string) instead of rendering nothing.
    assert_eq!(resolve_conversation_directory_path(Some(None), None), None);
}
