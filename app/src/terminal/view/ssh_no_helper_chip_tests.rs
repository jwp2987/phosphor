//! Unit tests for the no-helper chip's text and the show/hide decision (the decision itself is
//! `crate::settings::no_helper_chip_visible`, tested next to the settings).

use super::*;

#[test]
fn label_is_the_agreed_wording() {
    assert_eq!(CHIP_LABEL, "limited: no remote helper");
}

#[test]
fn explanation_says_the_session_works_and_never_nags() {
    assert!(CHIP_EXPLANATION.contains("works without"));
    assert!(CHIP_EXPLANATION.contains("ssh_extension_install_mode"));
    // Informational: no call to action that blocks, no warning wording.
    for word in ["error", "failed", "warning"] {
        assert!(!CHIP_EXPLANATION.to_lowercase().contains(word), "{word}");
    }
}

#[test]
fn chip_starts_collapsed() {
    let chip = SshNoHelperChip::new(SessionId::from(1_u64), "box1".to_owned());
    assert!(!chip.is_expanded());
}

/// The helper is limited today, so the text must not promise what it lacks (file tree, buffer
/// sync, diff panels, indexing, the agent file tools) and must say it is partial.
#[test]
fn explanation_does_not_promise_features_the_helper_lacks() {
    let text = CHIP_EXPLANATION.to_lowercase();
    for lacking in [
        "file browsing",
        "file tree",
        "richer",
        "code review",
        "index",
    ] {
        assert!(!text.contains(lacking), "{lacking}");
    }
    assert!(text.contains("only adds") && text.contains("more is planned"));
}
