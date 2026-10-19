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

#[cfg(unix)]
#[test]
fn private_socket_dir_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::tempdir().unwrap();
    let dir = private_socket_dir(base.path().to_path_buf());
    assert_ne!(dir, base.path());
    assert!(dir.starts_with(base.path()));
    assert_eq!(dir.metadata().unwrap().permissions().mode() & 0o777, 0o700);
    // Reusing an existing one works.
    assert_eq!(private_socket_dir(base.path().to_path_buf()), dir);
}

#[test]
fn dir_is_writable_probes_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    assert!(dir_is_writable(dir.path()));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    assert!(!dir_is_writable(&dir.path().join("missing")));
}
