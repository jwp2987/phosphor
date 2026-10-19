use super::*;

#[test]
fn usable_state_dir_is_used_and_created() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("a").join("state");
    assert_eq!(socket_dir(state.clone(), dir.path().join("tmp")), state);
    assert!(state.is_dir());
}

#[test]
fn state_dir_that_is_a_file_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    std::fs::write(&state, b"not a directory").unwrap();
    let fallback = dir.path().join("tmp");
    assert_eq!(socket_dir(state.clone(), fallback.clone()), fallback);
    assert!(state.is_file(), "the user's file is left alone");
}

#[cfg(unix)]
#[test]
fn read_only_state_dir_falls_back() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    std::fs::create_dir(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o555)).unwrap();
    let fallback = dir.path().join("tmp");
    let chosen = socket_dir(state.clone(), fallback.clone());
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(chosen, fallback);
}
