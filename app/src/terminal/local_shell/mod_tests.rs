use super::{InteractiveCaptureTimedOut, InteractiveEnvState, state_after_capture, PATH_CAPTURE_END, PATH_CAPTURE_START, extract_captured_path};

#[test]
fn extracts_clean_path() {
    let output = format!("{PATH_CAPTURE_START}/opt/homebrew/bin:/usr/bin{PATH_CAPTURE_END}");
    assert_eq!(
        extract_captured_path(&output),
        Some("/opt/homebrew/bin:/usr/bin")
    );
}

#[test]
fn ignores_startup_banner_output() {
    // rc files printing to stdout (fastfetch/MOTD) before the PATH line.
    let output = format!(
        "ascii art line 1\nOS macOS shell zsh\n\
         {PATH_CAPTURE_START}/opt/homebrew/bin:/usr/bin:/bin{PATH_CAPTURE_END}\n"
    );
    assert_eq!(
        extract_captured_path(&output),
        Some("/opt/homebrew/bin:/usr/bin:/bin")
    );
}

#[test]
fn missing_markers_returns_none() {
    assert_eq!(extract_captured_path("/opt/homebrew/bin:/usr/bin"), None);
}

#[test]
fn missing_end_marker_returns_none() {
    let output = format!("{PATH_CAPTURE_START}/opt/homebrew/bin");
    assert_eq!(extract_captured_path(&output), None);
}

#[test]
fn empty_path_between_markers() {
    let output = format!("{PATH_CAPTURE_START}{PATH_CAPTURE_END}");
    assert_eq!(extract_captured_path(&output), Some(""));
}

#[test]
fn preserves_colons_and_surrounding_noise() {
    let path = "/a:/b:/c";
    let output = format!("before{PATH_CAPTURE_START}{path}{PATH_CAPTURE_END}after");
    assert_eq!(extract_captured_path(&output), Some(path));
}

#[test]
fn timeout_does_not_poison_later_requests() {
    let (path, state) = state_after_capture(Err(anyhow::Error::new(InteractiveCaptureTimedOut)));
    assert_eq!(path, None);
    assert!(matches!(state, InteractiveEnvState::NotRequested));
}

#[test]
fn success_and_other_errors_are_cached() {
    let (path, state) = state_after_capture(Ok("/a:/b".to_owned()));
    assert_eq!(path.as_deref(), Some("/a:/b"));
    assert!(matches!(state, InteractiveEnvState::Ready(Some(_))));
    let (_, state) = state_after_capture(Err(anyhow::anyhow!("missing markers")));
    assert!(matches!(state, InteractiveEnvState::Ready(None)));
}
